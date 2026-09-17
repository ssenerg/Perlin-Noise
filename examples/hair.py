"""Grow a coat of hair from particles set loose in a 2D field.

Each particle is pulled by the lattice vectors around it. The path it takes
is one strand. The helper covers the usual knobs; :class:`Hair` is for the
rest (``drag``, ``thickness``, ``taper``, …).

From the repo (``just python`` installs into ``.venv``, not system Python)::

    just python-hair
    source .venv/bin/activate && python examples/hair.py images
"""

from __future__ import annotations

import sys
from pathlib import Path

from perlin_noise import Hair, Instance, Palette, Relief, Renderer, Shape, hair


def write(image, out: Path, name: str, note: str) -> None:
    path = out / name
    image.write_png(path)
    print(f"{name}: {image.width}x{image.height} {note}")


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "images")
    out.mkdir(parents=True, exist_ok=True)
    seed = 7
    size = 720
    # Density is count against the pixel count. The default 45_000 is pitched
    # at 1024²; scale it with the picture so the coat stays as thick.
    count = 22_000

    # The helper: grey coat, then the same coat painted with the field's
    # relief. ``palette`` without ``color_from_field`` is the ramp from dark
    # hair to a strand in full sheen; with it, the ramp the height map uses.
    write(
        hair(cells=5, seed=seed, size=size, count=count),
        out,
        "hair-gray.png",
        "grey coat",
    )
    write(
        hair(
            cells=5,
            seed=seed,
            size=size,
            count=count,
            color_from_field=True,
            palette=Palette.AZURE,
        ),
        out,
        "hair-azure.png",
        "coat coloured by azure relief",
    )

    # Hair() when you need knobs the helper does not list. ``swirl`` near 90
    # sends particles round the field instead of into it; ``frizz`` at 0
    # lays every strand perfectly parallel.
    combed = Hair(
        count=count,
        swirl=88,
        frizz=0.04,
        drag=4.5,
        thickness=1.1,
        jitter=0.45,
        seed=seed,
    )
    write(
        hair(cells=5, seed=seed, size=size, surface=combed),
        out,
        "hair-combed.png",
        "high swirl, low frizz",
    )

    # Same field, same particles, through Renderer rather than the helper.
    # ``surface`` here is the Relief the colour map is drawn with.
    field = Renderer(Instance([5, 5], seed))
    settings = Hair(
        count=count,
        color_from_field=True,
        surface=Relief(shape=Shape.BILLOW, palette=Palette.CRIMSON, height=0.12),
        seed=seed,
    )
    image = field.hair(size, size, settings)
    write(image, out, "hair-relief.png", "Renderer.hair with a custom Relief map")


if __name__ == "__main__":
    main()
