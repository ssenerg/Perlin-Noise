// One invocation per intersection of the refined lattice.
//
// `axes` packs four arrays of `n_dims` values each, in this order:
//   dims             cells per axis of the coarse lattice
//   divisors         how many pieces each coarse cell is cut into
//   table_strides    row-major strides over the refined lattice
//   lattice_strides  row-major strides over the coarse lattice

struct Params {
    n_dims: u32,
    n_points: u32,
    point_offset: u32,
    n_corners: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> axes: array<u32>;
@group(0) @binding(2) var<storage, read> grid: array<f32>;
@group(0) @binding(3) var<storage, read_write> values: array<f32>;

fn fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

@compute @workgroup_size(64)
fn table(@builtin(global_invocation_id) id: vec3<u32>) {
    let flat = id.x + params.point_offset;
    if (flat >= params.n_points) {
        return;
    }

    let n = params.n_dims;
    // Fixed capacity, matching MAX_DIMS on the Rust side.
    var cell: array<u32, 16>;
    var offset: array<f32, 16>;
    var eased: array<f32, 16>;

    var rest = flat;
    for (var i = 0u; i < n; i = i + 1u) {
        let table_stride = axes[2u * n + i];
        let index = rest / table_stride;
        rest = rest % table_stride;

        let divisor = axes[n + i];
        let dim = axes[i];
        // Integer division keeps the cell exact, so points sitting on a coarse
        // intersection get offset 0 or 1 with no rounding slack.
        var base = index / divisor;
        var local = 0.0;
        if (base >= dim) {
            base = dim - 1u;
            local = 1.0;
        } else {
            local = f32(index % divisor) / f32(divisor);
        }

        cell[i] = base;
        offset[i] = local;
        eased[i] = fade(local);
    }

    var total = 0.0;
    for (var corner = 0u; corner < params.n_corners; corner = corner + 1u) {
        var weight = 1.0;
        var lattice = 0u;
        for (var i = 0u; i < n; i = i + 1u) {
            let bit = (corner >> i) & 1u;
            lattice = lattice + (cell[i] + bit) * axes[3u * n + i];
            if (bit == 1u) {
                weight = weight * eased[i];
            } else {
                weight = weight * (1.0 - eased[i]);
            }
        }

        var projection = 0.0;
        for (var i = 0u; i < n; i = i + 1u) {
            let bit = (corner >> i) & 1u;
            projection = projection + grid[lattice * n + i] * (offset[i] - f32(bit));
        }

        total = total + weight * projection;
    }

    values[flat] = total;
}
