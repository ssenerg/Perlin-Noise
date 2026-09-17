"""Write one image per rendering mode, the Python twin of ``examples/render.rs``.

The helpers (``grayscale``, ``relief``, ``hair``, …) build the field and
render in one call. ``Renderer`` is there when you want the pieces, or when
a mode has no helper (neighbouring colour slices, a single slice through a
ramp).

From the repo (``just python`` installs into ``.venv``, not system Python)::

    just python-render
    source .venv/bin/activate && python examples/render.py images
"""

from __future__ import annotations

import sys
from pathlib import Path

from perlin_noise import (
    Globe,
    Hair,
    Instance,
    Palette,
    Relief,
    Renderer,
    Shape,
    channels,
    colored,
    globe,
    grayscale,
    hair,
    relief,
)


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "images")
    out.mkdir(parents=True, exist_ok=True)
    seed = 20260915

    # Helpers: 8x8 cells sampled 64 times per cell, so a 513x513 image.
    gray = grayscale(cells=8, seed=seed, detail=64)
    gray.write_png(out / "noise-gray.png")
    print(f"noise-gray.png: {gray.width}x{gray.height} greyscale")

    # Three slices of a 3D field, one each for red, green and blue.
    # Neighbouring slices give colours that drift; spreading them over the
    # whole axis (the default) gives vivid unrelated channels.
    vivid = channels(cells=8, seed=seed, detail=64, depth=8)
    vivid.write_png(out / "noise-channels.png")
    print(f"noise-channels.png: {vivid.width}x{vivid.height} from 3 spread out slices")

    # One cell is enough for the colour axis: it only has to be deep enough
    # for the slices the channels come from.
    deep = Renderer(Instance([8, 8, 1], seed))

    # Neighbouring slices instead, an eighth of a cell apart.
    soft = deep.rgb_channels_at([64, 64, 8], (0, 1, 2))
    soft.write_png(out / "noise-channels-soft.png")
    print(
        f"noise-channels-soft.png: {soft.width}x{soft.height} from 3 neighbouring slices"
    )

    # One slice of the colour axis, coloured by a ramp rather than by channel.
    sliced = deep.rgb_palette([64, 64, 1], 0, Palette.BLUE_RED)
    sliced.write_png(out / "noise-bluered.png")
    print(f"noise-bluered.png: {sliced.width}x{sliced.height} from one slice through BLUE_RED")

    for palette, name in [
        (Palette.HEAT, "noise-heat.png"),
        (Palette.TERRAIN, "noise-terrain.png"),
    ]:
        image = colored(cells=8, seed=seed, detail=64, palette=palette)
        image.write_png(out / name)
        print(f"{name}: {image.width}x{image.height} through {palette}")

    # The same field read as a height map and lit, once per shape.
    for shape, name in [
        (Shape.BILLOW, "noise-relief-billow.png"),
        (Shape.RIDGED, "noise-relief-ridged.png"),
        (Shape.SMOOTH, "noise-relief-smooth.png"),
    ]:
        image = relief(cells=8, seed=seed, detail=64, shape=shape)
        image.write_png(out / name)
        print(f"{name}: {image.width}x{image.height} lit as {shape}")

    # Or pass a ready-made Relief when you want several lighting flags at once.
    glossy = relief(
        cells=8,
        seed=seed,
        detail=64,
        surface=Relief(shape=Shape.BILLOW, palette=Palette.AZURE, gloss=48),
    )
    glossy.write_png(out / "noise-relief-azure.png")
    print(f"noise-relief-azure.png: {glossy.width}x{glossy.height} azure, tight highlight")

    # Wide cells, so a strand has room to run before the field turns it.
    # A quarter of the pixels the default count is pitched at, so a quarter
    # of the strands keeps the coat as dense.
    coat = Hair(count=11_000)
    for settings, name in [
        (coat, "noise-hair.png"),
        (Hair(count=11_000, color_from_field=True), "noise-hair-relief.png"),
    ]:
        image = hair(cells=5, seed=seed, size=513, surface=settings)
        image.write_png(out / name)
        colour = "the relief" if settings.color_from_field else settings.palette
        print(
            f"{name}: {image.width}x{image.height} from {settings.count} particles through {colour}"
        )

    # A sphere out of a four dimensional field: three axes hold it, the
    # fourth carries its height and colour. Its size is in pixels, since it
    # samples a surface rather than a lattice.
    for settings, name in [
        (Globe(), "noise-globe.png"),
        (Globe(rgb_from_fourth_axis=False), "noise-globe-crimson.png"),
    ]:
        image = globe(cells=7, seed=seed, size=513, depth=2, surface=settings)
        image.write_png(out / name)
        colour = (
            "the fourth axis"
            if settings.rgb_from_fourth_axis
            else settings.surface.palette
        )
        print(f"{name}: {image.width}x{image.height} sphere, colour from {colour}")


if __name__ == "__main__":
    main()
