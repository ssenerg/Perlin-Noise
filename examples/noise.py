"""Sample a Perlin field from Python.

``dims`` counts *cells* per axis, so ``[3, 1, 2]`` is a 3x1x2 grid with
24 intersections. The same ``seed`` always rebuilds the same lattice.

From the repo (``just python`` installs into ``.venv``, not system Python)::

    just python-noise
    source .venv/bin/activate && python examples/noise.py
"""

from perlin_noise import MAX_DIMS, Instance


def main() -> None:
    noise = Instance([3, 1, 2], seed=42)
    print(f"dims {noise.dims}  shape {noise.shape}  intersections {noise.intersections}")
    print(f"seed {noise.seed}  MAX_DIMS {MAX_DIMS}")

    # One point, anywhere inside [0, 3] x [0, 1] x [0, 2]. Values are raw
    # Perlin: centred on zero, bounded by ±sqrt(n), and exactly zero at every
    # intersection of the original lattice.
    value = noise.noise([1.5, 0.25, 2.0])
    print(f"noise([1.5, 0.25, 2.0]) = {value:.6f}")

    # The same blend, but of the corner gradients rather than their dot
    # products: the force a particle in hair mode is pulled by.
    pull = noise.force([1.5, 0.25, 2.0])
    print(f"force([1.5, 0.25, 2.0]) = {[round(c, 6) for c in pull]}")

    # Or every intersection of a finer lattice: one divisor per axis says
    # how many pieces each cell is cut into, so this is a 7 x 2 x 9 table.
    table = noise.table([2, 1, 4])
    assert table.shape == [7, 2, 9]
    assert len(table) == 7 * 2 * 9
    print(f"table shape {table.shape}, {len(table)} values, corner {table.get([0, 0, 0])}")

    # Sampling a point outside [0, dims[i]] raises ValueError, as does the
    # wrong number of coordinates.
    try:
        noise.noise([4.0, 0.0, 0.0])
    except ValueError as err:
        print(f"out of range: {err}")


if __name__ == "__main__":
    main()
