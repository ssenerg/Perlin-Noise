//! Perlin noise in an arbitrary number of dimensions.
//!
//! An [`Instance`] owns a lattice whose shape is described by `dims`: `dims`
//! counts *cells* per axis, so `[3, 1, 2]` is a 3x1x2 grid holding
//! `4 * 2 * 3 = 24` intersections. Every intersection carries a random unit
//! vector of length `dims.len()`, derived from the seed given to
//! [`Instance::new`], so the same seed always rebuilds the same field.
//!
//! Two ways to sample it:
//!
//! * [`Instance::noise`] for a single point inside the lattice, i.e. inside
//!   `[0, dims[0]] x [0, dims[1]] x ...`.
//! * [`Instance::table`] for every intersection of a refined lattice, where
//!   each axis of `dims` is subdivided by the matching divisor.
//!
//! ```
//! use perlin_noise::Instance;
//!
//! let noise = Instance::new(vec![3, 1, 2], 42)?;
//! let value = noise.noise(&[1.5, 0.25, 2.0])?;
//! assert!(value.abs() <= 3f64.sqrt());
//!
//! // Two extra samples per cell on the first axis, four on the last.
//! let table = noise.table(&[2, 1, 4])?;
//! assert_eq!(table.shape(), &[7, 2, 9]);
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! A [`Renderer`] wraps a field of two to four dimensions and samples it into
//! an [`Image`]: greyscale, colour, a lit height map, or with four dimensions
//! a lit sphere through [`Renderer::globe`].
//!
//! With the `gpu` feature enabled, [`Instance::table_gpu`] runs the same
//! computation as a compute shader through `wgpu`. With the `png` feature,
//! [`Image::write_png`] saves the result to disk.

use std::io::{Error, ErrorKind, Result};

#[cfg(feature = "gpu")]
mod gpu;
mod image;
mod rng;

pub use image::{Globe, Image, MAX_SAMPLES, Palette, Relief, Renderer, Shape};
use rng::SplitMix64;

/// Upper bound on `dims.len()`.
///
/// Every sample blends the `2^n` corners of the surrounding cell, so the cost
/// per point doubles with each dimension; the limit keeps that from silently
/// becoming unpayable and lets the hot loops use stack arrays.
pub const MAX_DIMS: usize = 16;

/// Number of points below which the table is computed on a single thread.
const PARALLEL_THRESHOLD: usize = 1 << 14;

/// Perlin's quintic ease curve, flat in both value and slope at 0 and 1.
#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Row-major strides for a shape, i.e. the last axis is contiguous.
fn row_major_strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = vec![1usize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        strides[i] = strides[i + 1] * shape[i + 1];
    }
    strides
}

fn overflow() -> Error {
    Error::new(ErrorKind::InvalidInput, "Grid size overflows usize")
}

/// A seeded Perlin noise field.
pub struct Instance {
    dims: Vec<usize>,
    /// Intersections per axis, `dims[i] + 1`.
    shape: Vec<usize>,
    /// Row-major strides over `shape`, in intersections.
    strides: Vec<usize>,
    /// `inters * dims.len()` gradient components, one unit vector per
    /// intersection, in row-major intersection order.
    grid: Vec<f64>,
    inters: usize,
    seed: u64,
}

impl Instance {
    /// Builds the lattice for `dims` and fills every intersection with a
    /// random unit vector derived from `seed`.
    pub fn new(dims: Vec<usize>, seed: u64) -> Result<Self> {
        if dims.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "At least one dimension is required",
            ));
        }
        if dims.len() > MAX_DIMS {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("At most {MAX_DIMS} dimensions are supported"),
            ));
        }
        for dim in &dims {
            if *dim == 0 {
                return Err(Error::new(ErrorKind::InvalidInput, "Dimension cannot be 0"));
            }
        }

        let n = dims.len();
        let shape: Vec<usize> = dims.iter().map(|dim| dim + 1).collect();
        let mut inters: usize = 1;
        for size in &shape {
            inters = inters.checked_mul(*size).ok_or_else(overflow)?;
        }
        let components = inters.checked_mul(n).ok_or_else(overflow)?;

        let mut generator = SplitMix64::new(seed);
        let mut grid: Vec<f64> = Vec::with_capacity(components);
        for _ in 0..inters {
            grid.extend_from_slice(&generator.next_unit_vector(n));
        }

        Ok(Self {
            strides: row_major_strides(&shape),
            dims,
            shape,
            grid,
            inters,
            seed,
        })
    }

    /// Cells per axis, as passed to [`Instance::new`].
    pub fn dims(&self) -> &[usize] {
        &self.dims
    }

    /// Intersections per axis, one more than the cell count.
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Total number of intersections.
    pub fn intersections(&self) -> usize {
        self.inters
    }

    /// The seed this field was built from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Gradient stored at an intersection, or `None` if `index` is out of the
    /// lattice.
    pub fn gradient(&self, index: &[usize]) -> Option<&[f64]> {
        if index.len() != self.dims.len() {
            return None;
        }
        let mut flat = 0usize;
        for (i, coord) in index.iter().enumerate() {
            if *coord >= self.shape[i] {
                return None;
            }
            flat += coord * self.strides[i];
        }
        let n = self.dims.len();
        Some(&self.grid[flat * n..flat * n + n])
    }

    /// Noise at a single point.
    ///
    /// `point` must have one coordinate per dimension and lie within
    /// `[0, dims[i]]` on every axis. The result is 0 exactly at the
    /// intersections and stays within `±sqrt(n)` elsewhere.
    pub fn noise(&self, point: &[f64]) -> Result<f64> {
        let n = self.dims.len();
        if point.len() != n {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Expected {n} coordinates, got {}", point.len()),
            ));
        }

        for (i, coord) in point.iter().enumerate() {
            let upper = self.dims[i] as f64;
            // Also rejects NaN, which fails every comparison.
            if !(*coord >= 0.0 && *coord <= upper) {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "Coordinate {i} is {coord}, outside the boundary [0, {}]",
                        self.dims[i]
                    ),
                ));
            }
        }

        Ok(self.sample(point))
    }

    /// Noise at a point, with each coordinate pulled into the lattice instead
    /// of being rejected for falling outside it.
    ///
    /// `point` must still carry one coordinate per dimension. This is what
    /// callers that walk a surface through the field use, where a coordinate
    /// can land a rounding error past the boundary.
    pub(crate) fn sample(&self, point: &[f64]) -> f64 {
        let n = self.dims.len();
        let mut cell = [0usize; MAX_DIMS];
        let mut offset = [0f64; MAX_DIMS];
        for i in 0..n {
            let coord = point[i].clamp(0.0, self.dims[i] as f64);
            // The upper boundary belongs to the last cell, at local offset 1.
            let base = (coord.floor() as usize).min(self.dims[i] - 1);
            cell[i] = base;
            offset[i] = coord - base as f64;
        }

        self.eval(&cell[..n], &offset[..n])
    }

    /// Noise on a refined lattice.
    ///
    /// `divisors` holds one positive integer per dimension telling how many
    /// pieces each cell of that axis is cut into, so axis `i` ends up with
    /// `dims[i] * divisors[i] + 1` intersections. The returned [`Table`]
    /// carries the noise value at each of them.
    pub fn table(&self, divisors: &[usize]) -> Result<Table> {
        let (shape, count) = self.table_layout(divisors)?;
        let strides = row_major_strides(&shape);
        let mut values = vec![0f64; count];

        if count < PARALLEL_THRESHOLD {
            for (flat, value) in values.iter_mut().enumerate() {
                *value = self.eval_table_point(flat, divisors, &strides);
            }
        } else {
            let threads = std::thread::available_parallelism()
                .map(|value| value.get())
                .unwrap_or(1)
                .clamp(1, count.div_ceil(PARALLEL_THRESHOLD));
            let chunk = count.div_ceil(threads);
            std::thread::scope(|scope| {
                for (index, part) in values.chunks_mut(chunk).enumerate() {
                    let strides = &strides;
                    let start = index * chunk;
                    scope.spawn(move || {
                        for (offset, value) in part.iter_mut().enumerate() {
                            *value = self.eval_table_point(start + offset, divisors, strides);
                        }
                    });
                }
            });
        }

        Ok(Table { shape, values })
    }

    /// Same as [`Instance::table`], evaluated by a compute shader.
    ///
    /// Requires the `gpu` feature. The shader works in 32 bit floats, so
    /// values differ from the CPU path by a small rounding error. Falls back to
    /// an error if no adapter is available, in which case [`Instance::table`]
    /// still works.
    #[cfg(feature = "gpu")]
    pub fn table_gpu(&self, divisors: &[usize]) -> Result<Table> {
        let (shape, count) = self.table_layout(divisors)?;
        let strides = row_major_strides(&shape);
        let values = gpu::run(self, divisors, &strides, count)?;
        Ok(Table { shape, values })
    }

    /// Intersections per axis that [`Instance::table`] would produce for
    /// `divisors`, without computing any noise.
    pub fn table_shape(&self, divisors: &[usize]) -> Result<Vec<usize>> {
        Ok(self.table_layout(divisors)?.0)
    }

    /// Validates `divisors` and returns the refined shape with its point count.
    fn table_layout(&self, divisors: &[usize]) -> Result<(Vec<usize>, usize)> {
        let n = self.dims.len();
        if divisors.len() != n {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Expected {n} divisors, got {}", divisors.len()),
            ));
        }

        let mut shape = Vec::with_capacity(n);
        let mut count: usize = 1;
        for (i, divisor) in divisors.iter().enumerate() {
            if *divisor == 0 {
                return Err(Error::new(ErrorKind::InvalidInput, "Divisor cannot be 0"));
            }
            let size = self.dims[i]
                .checked_mul(*divisor)
                .and_then(|cells| cells.checked_add(1))
                .ok_or_else(overflow)?;
            count = count.checked_mul(size).ok_or_else(overflow)?;
            shape.push(size);
        }

        Ok((shape, count))
    }

    /// Noise at the refined-lattice point with row-major index `flat`.
    fn eval_table_point(&self, flat: usize, divisors: &[usize], strides: &[usize]) -> f64 {
        let n = self.dims.len();
        let mut cell = [0usize; MAX_DIMS];
        let mut offset = [0f64; MAX_DIMS];

        let mut rest = flat;
        for i in 0..n {
            let index = rest / strides[i];
            rest %= strides[i];
            // Integer division keeps the cell exact, so points that land on an
            // intersection get offset 0 or 1 with no rounding slack.
            let base = index / divisors[i];
            if base >= self.dims[i] {
                cell[i] = self.dims[i] - 1;
                offset[i] = 1.0;
            } else {
                cell[i] = base;
                offset[i] = (index % divisors[i]) as f64 / divisors[i] as f64;
            }
        }

        self.eval(&cell[..n], &offset[..n])
    }

    /// Blends the `2^n` corners of `cell` at local position `offset`.
    fn eval(&self, cell: &[usize], offset: &[f64]) -> f64 {
        let n = cell.len();
        let mut eased = [0f64; MAX_DIMS];
        for i in 0..n {
            eased[i] = fade(offset[i]);
        }

        let mut total = 0f64;
        for corner in 0..(1usize << n) {
            let mut weight = 1f64;
            let mut flat = 0usize;
            for i in 0..n {
                let step = (corner >> i) & 1;
                flat += (cell[i] + step) * self.strides[i];
                weight *= if step == 1 { eased[i] } else { 1.0 - eased[i] };
            }
            if weight == 0.0 {
                continue;
            }

            let gradient = &self.grid[flat * n..flat * n + n];
            let mut dot = 0f64;
            for i in 0..n {
                dot += gradient[i] * (offset[i] - ((corner >> i) & 1) as f64);
            }
            total += weight * dot;
        }

        total
    }

    /// Gradient components, row-major per intersection.
    #[cfg(feature = "gpu")]
    pub(crate) fn grid(&self) -> &[f64] {
        &self.grid
    }

    /// Row-major strides over the intersection lattice.
    #[cfg(feature = "gpu")]
    pub(crate) fn lattice_strides(&self) -> &[usize] {
        &self.strides
    }
}

/// Noise values sampled on a refined lattice, in row-major order.
pub struct Table {
    shape: Vec<usize>,
    values: Vec<f64>,
}

impl Table {
    /// Intersections per axis.
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// All values, row-major: the last axis varies fastest.
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Consumes the table and returns its values.
    pub fn into_values(self) -> Vec<f64> {
        self.values
    }

    /// Value at a lattice index, or `None` if the index is out of the table.
    pub fn get(&self, index: &[usize]) -> Option<f64> {
        if index.len() != self.shape.len() {
            return None;
        }
        let mut flat = 0usize;
        for (coord, size) in index.iter().zip(&self.shape) {
            if coord >= size {
                return None;
            }
            flat = flat * size + coord;
        }
        Some(self.values[flat])
    }
}
