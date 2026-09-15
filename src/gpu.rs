//! GPU backend for [`crate::Instance::table_gpu`].
//!
//! The lattice gradients are uploaded once per call and the refined lattice is
//! evaluated by `table.wgsl`, one invocation per intersection. WGSL has no 64
//! bit floats, so the shader works in `f32` and the results are widened on the
//! way back.

use std::io::{Error, ErrorKind, Result};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::Instance;

/// Matches `MAX_DIMS` in `table.wgsl`.
const SHADER_MAX_DIMS: usize = 16;

const WORKGROUP_SIZE: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    n_dims: u32,
    n_points: u32,
    point_offset: u32,
    n_corners: u32,
}

fn unsupported(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::Unsupported, message.into())
}

/// Evaluates the refined lattice on the GPU and returns its values in
/// row-major order.
pub(crate) fn run(
    instance: &Instance,
    divisors: &[usize],
    table_strides: &[usize],
    count: usize,
) -> Result<Vec<f64>> {
    let n = instance.dims().len();
    if n > SHADER_MAX_DIMS {
        return Err(unsupported(format!(
            "The shader supports at most {SHADER_MAX_DIMS} dimensions"
        )));
    }
    // The shader addresses points and lattice entries with 32 bit integers.
    let n_points = u32::try_from(count)
        .map_err(|_| unsupported("Table has more points than a shader invocation can index"))?;

    let values_bytes = (count * size_of::<f32>()) as u64;
    let grid_bytes = (instance.grid().len() * size_of::<f32>()) as u64;

    let wgpu_instance = wgpu::Instance::default();
    let adapter = pollster::block_on(wgpu_instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .map_err(|error| unsupported(format!("No GPU adapter available: {error}")))?;

    let limits = adapter.limits();
    let budget = limits.max_storage_buffer_binding_size;
    for (what, needed) in [("output", values_bytes), ("gradient", grid_bytes)] {
        if needed > budget {
            return Err(unsupported(format!(
                "The {what} buffer needs {needed} bytes but the adapter caps storage buffers at {budget}"
            )));
        }
    }

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("perlin-noise"),
        required_limits: limits.clone(),
        ..Default::default()
    }))
    .map_err(|error| unsupported(format!("Could not open the GPU device: {error}")))?;

    // Four arrays of `n` values, laid out as the shader's `axes` binding.
    let mut axes: Vec<u32> = Vec::with_capacity(4 * n);
    for group in [
        instance.dims(),
        divisors,
        table_strides,
        instance.lattice_strides(),
    ] {
        for value in group {
            axes.push(u32::try_from(*value).map_err(|_| {
                unsupported("Lattice description does not fit in 32 bit shader integers")
            })?);
        }
    }

    let grid: Vec<f32> = instance.grid().iter().map(|value| *value as f32).collect();

    let axes_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("perlin-noise-axes"),
        contents: bytemuck::cast_slice(&axes),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let grid_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("perlin-noise-grid"),
        contents: bytemuck::cast_slice(&grid),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let values_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("perlin-noise-values"),
        size: values_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("perlin-noise-staging"),
        size: values_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("perlin-noise-table"),
        source: wgpu::ShaderSource::Wgsl(include_str!("table.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("perlin-noise-table"),
        layout: None,
        module: &shader,
        entry_point: Some("table"),
        compilation_options: Default::default(),
        cache: None,
    });
    let layout = pipeline.get_bind_group_layout(0);

    // A dispatch is capped in workgroups per dimension, so large tables are
    // covered by several dispatches, each with its own starting point.
    let stride = limits
        .max_compute_workgroups_per_dimension
        .saturating_mul(WORKGROUP_SIZE);
    let dispatches = n_points.div_ceil(stride).max(1);

    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("perlin-noise"),
    });
    let mut bind_groups = Vec::with_capacity(dispatches as usize);
    for dispatch in 0..dispatches {
        let point_offset = dispatch * stride;
        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("perlin-noise-params"),
            contents: bytemuck::bytes_of(&Params {
                n_dims: n as u32,
                n_points,
                point_offset,
                n_corners: 1u32 << n,
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("perlin-noise"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: axes_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: grid_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: values_buffer.as_entire_binding(),
                },
            ],
        }));
    }

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("perlin-noise-table"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        for (dispatch, bind_group) in bind_groups.iter().enumerate() {
            let point_offset = dispatch as u32 * stride;
            let remaining = n_points - point_offset;
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(remaining.min(stride).div_ceil(WORKGROUP_SIZE), 1, 1);
        }
    }
    encoder.copy_buffer_to_buffer(&values_buffer, 0, &staging_buffer, 0, values_bytes);
    queue.submit(Some(encoder.finish()));

    let (sender, receiver) = std::sync::mpsc::channel();
    staging_buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| unsupported(format!("Waiting on the GPU failed: {error}")))?;

    if let Some(error) = pollster::block_on(error_scope.pop()) {
        return Err(unsupported(format!("The compute pass failed: {error}")));
    }
    receiver
        .recv()
        .map_err(|_| unsupported("The GPU dropped the readback callback"))?
        .map_err(|error| unsupported(format!("Could not read the result buffer: {error}")))?;

    let view = staging_buffer
        .slice(..)
        .get_mapped_range()
        .map_err(|error| unsupported(format!("Could not map the result buffer: {error}")))?;
    let values: Vec<f64> = bytemuck::cast_slice::<u8, f32>(&view)
        .iter()
        .map(|value| f64::from(*value))
        .collect();
    drop(view);
    staging_buffer.unmap();

    Ok(values)
}
