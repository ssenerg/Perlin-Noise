"""Perlin noise in any number of dimensions, and pictures of it.

Install with ``pip install perlin-noise-rs``, or ``maturin develop`` from
the repo. After that, ``help(perlin_noise)`` and ``help(perlin_noise.Instance)``
are the docs: every public type and helper below has a docstring.

A field is an :class:`Instance`. ``dims`` counts *cells* per axis, so
``[8, 8]`` is an 8×8 grid. The same ``seed`` always rebuilds the same lattice.

::

    from perlin_noise import Instance

    noise = Instance([3, 1, 2], seed=42)
    value = noise.noise([1.5, 0.25, 2.0])
    table = noise.table([2, 1, 4])
    assert table.shape == [7, 2, 9]

:class:`Renderer` turns a field of two to four dimensions into an
:class:`Image`. For the usual pictures you can skip it and call the helpers
on this module, which build the field, render, and give you the image:

::

    from perlin_noise import relief, hair, globe, Palette

    relief(seed=7, cells=12).write_png("relief.png")
    hair(seed=7, size=1440).write_png("hair.png")
    hair(seed=7, size=1440, color_from_field=True).write_png("hair-color.png")
    globe(seed=3, palette=Palette.CRIMSON).write_png("globe.png")

Sampling a point outside ``[0, dims[i]]`` raises ``ValueError``, as does a
``NaN`` coordinate or the wrong number of them. Values are raw Perlin:
centred on zero, bounded by ``±sqrt(n)``, and exactly zero at every
intersection of the original lattice.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TypeAlias

from perlin_noise._native import (
    MAX_DIMS as MAX_DIMS,
    MAX_SAMPLES as MAX_SAMPLES,
    Globe as Globe,
    Hair as Hair,
    Image as Image,
    Instance as Instance,
    Palette as Palette,
    Relief as Relief,
    Renderer as Renderer,
    Shape as Shape,
    Table as Table,
)

__all__ = [
    "MAX_DIMS",
    "MAX_SAMPLES",
    "Globe",
    "Hair",
    "Image",
    "Instance",
    "Palette",
    "Relief",
    "Renderer",
    "Shape",
    "Table",
    "channels",
    "colored",
    "globe",
    "grayscale",
    "hair",
    "relief",
]

Size: TypeAlias = int | tuple[int, int] | list[int]


def _pair(value: Size, default: int | None = None) -> list[int]:
    if isinstance(value, int):
        n = default if default is not None and value == 0 else value
        return [n, n]
    axes = [int(part) for part in value]
    if len(axes) == 1:
        return [axes[0], axes[0]]
    if len(axes) != 2:
        raise ValueError(f"expected one size or two, got {value!r}")
    return axes


def _cells(cells: Size) -> list[int]:
    return _pair(cells)


def grayscale(
    cells: Size = 8,
    seed: int = 1,
    detail: Size = 64,
) -> Image:
    """Greyscale picture of a two dimensional field.

    Parameters
    ----------
    cells:
        Noise cells per axis. One number makes a square; ``(down, across)``
        sets the two axes separately.
    seed:
        Lattice seed. The same seed always rebuilds the same field.
    detail:
        Samples per cell. The image comes out ``cells * detail + 1`` pixels
        on each axis.

    Returns
    -------
    Image
        One byte per pixel, darkest to brightest sample stretched over 0..255.
    """
    dims = _cells(cells)
    divisors = _pair(detail)
    return Renderer(Instance(dims, seed)).grayscale(divisors)


def colored(
    cells: Size = 8,
    seed: int = 1,
    detail: Size = 64,
    palette: Palette = Palette.TERRAIN,
) -> Image:
    """A two dimensional field run through ``palette``.

    Parameters
    ----------
    cells:
        Noise cells per axis. One number makes a square.
    seed:
        Lattice seed.
    detail:
        Samples per cell.
    palette:
        Colour ramp. :attr:`Palette.TERRAIN` is the default here because a
        grey ramp is what :func:`grayscale` is for.

    Returns
    -------
    Image
        Three bytes per pixel.
    """
    dims = _cells(cells)
    divisors = _pair(detail)
    return Renderer(Instance(dims, seed)).colored(divisors, palette)


def channels(
    cells: Size = 8,
    seed: int = 1,
    detail: Size = 64,
    depth: int = 8,
) -> Image:
    """Three slices of a 3D field, one each for red, green and blue.

    Neighbouring slices give colours that drift; spreading them over the
    whole axis (the default) gives vivid unrelated channels. The colour axis
    is one cell deep, which is enough for the slices.

    Parameters
    ----------
    cells:
        Cells of the two spatial axes.
    seed:
        Lattice seed.
    detail:
        Samples per spatial cell.
    depth:
        Samples along the colour axis, at least 3.

    Returns
    -------
    Image
        Three bytes per pixel.
    """
    spatial = _cells(cells)
    samples = _pair(detail)
    noise = Instance([spatial[0], spatial[1], 1], seed)
    return Renderer(noise).rgb_channels([samples[0], samples[1], depth])


def relief(
    cells: Size = 8,
    seed: int = 1,
    detail: Size = 64,
    *,
    shape: Shape | None = None,
    palette: Palette | None = None,
    light: Sequence[float] | None = None,
    height: float | None = None,
    gamma: float | None = None,
    occlusion: float | None = None,
    ambient: float | None = None,
    diffuse: float | None = None,
    specular: float | None = None,
    gloss: float | None = None,
    surface: Relief | None = None,
) -> Image:
    """A two dimensional field read as a height map and lit.

    This is the glossy liquid look. Pass keyword lighting flags, or a
    ready-made :class:`Relief` as ``surface``. Keyword flags override
    ``surface`` where both are given.

    Parameters
    ----------
    cells:
        Noise cells per axis.
    seed:
        Lattice seed.
    detail:
        Samples per cell. The image comes out ``cells * detail + 1`` pixels
        on each axis.
    shape:
        :attr:`Shape.SMOOTH`, :attr:`Shape.BILLOW` or :attr:`Shape.RIDGED`.
    palette:
        Colour ramp. :attr:`Palette.CRIMSON` and :attr:`Palette.AZURE` are
        the two built for this mode: they hold no black, so the dark comes
        only from the shading.
    light:
        ``(x, y, z)`` in image space: x right, y down, z towards the viewer.
    height:
        How far the height map is exaggerated before slopes are measured.
    gamma:
        Height curve. Above 1.0 opens the creases into wider basins.
    occlusion:
        How dark the low ground goes, 0 to 1.
    ambient, diffuse, specular, gloss:
        Lighting. ``gloss`` is the tightness of the highlight.
    surface:
        A :class:`Relief` to start from. Defaults to the glossy crimson look.

    Returns
    -------
    Image
        Three bytes per pixel.
    """
    settings = surface if surface is not None else Relief()
    if shape is not None:
        settings.shape = shape
    if palette is not None:
        settings.palette = palette
    if light is not None:
        settings.light = list(light)
    if height is not None:
        settings.height = height
    if gamma is not None:
        settings.gamma = gamma
    if occlusion is not None:
        settings.occlusion = occlusion
    if ambient is not None:
        settings.ambient = ambient
    if diffuse is not None:
        settings.diffuse = diffuse
    if specular is not None:
        settings.specular = specular
    if gloss is not None:
        settings.gloss = gloss
    dims = _cells(cells)
    divisors = _pair(detail)
    return Renderer(Instance(dims, seed)).relief(divisors, settings)


def globe(
    cells: Size = 7,
    seed: int = 1,
    size: Size = 1024,
    depth: int = 2,
    *,
    palette: Palette | None = None,
    fill: float | None = None,
    samples: int | None = None,
    surface: Globe | None = None,
) -> Image:
    """A sphere carved out of a four dimensional field.

    Three axes hold the sphere; the fourth is sampled for height and colour.
    ``palette`` colours the sphere by height instead of taking RGB off that
    fourth axis.

    Parameters
    ----------
    cells:
        Cells across the sphere. Pass ``(across, depth)`` to set the fourth
        axis as well; otherwise ``depth`` is used.
    seed:
        Lattice seed.
    size:
        Image in pixels. One number makes a square.
    depth:
        Cells along the fourth axis when ``cells`` is a single number.
    palette:
        If given, colour by height through this ramp.
    fill:
        How much of the shorter side the sphere fills, above 0 and at most 1.
    samples:
        Shading samples per pixel along each axis. 2 shades each pixel four
        times; 1 leaves the highlights speckled.
    surface:
        A :class:`Globe` to start from.

    Returns
    -------
    Image
        Three bytes per pixel. ``divisors`` play no part: ``cells`` alone set
        the feature size.
    """
    if isinstance(cells, int):
        across, axis = cells, depth
    else:
        axes = [int(part) for part in cells]
        if len(axes) == 1:
            across, axis = axes[0], depth
        elif len(axes) == 2:
            across, axis = axes[0], axes[1]
        else:
            raise ValueError(f"cells wants one value or two, got {cells!r}")
    settings = surface if surface is not None else Globe()
    if palette is not None:
        settings.rgb_from_fourth_axis = False
        surface_relief = settings.surface
        surface_relief.palette = palette
        settings.surface = surface_relief
    if fill is not None:
        settings.fill = fill
    if samples is not None:
        settings.samples = samples
    pixels = _pair(size)
    noise = Instance([across, across, across, axis], seed)
    return Renderer(noise).globe(pixels[0], pixels[1], settings)


def hair(
    cells: Size = 5,
    seed: int = 1,
    size: Size = 1024,
    *,
    count: int | None = None,
    steps: int | None = None,
    color_from_field: bool = False,
    palette: Palette | None = None,
    frizz: float | None = None,
    swirl: float | None = None,
    surface: Hair | None = None,
) -> Image:
    """A coat of hair grown by setting particles loose in a 2D field.

    Each particle is pulled by the lattice vectors around it. The path it
    takes is one strand. ``color_from_field`` paints the coat with the
    relief of the same field (the crimson liquid look by default).

    Parameters
    ----------
    cells:
        Noise cells per axis. Wider cells give a strand more room to run.
    seed:
        Lattice seed, and the seed the particles start from.
    size:
        Image in pixels. One number makes a square.
    count:
        How many particles, one strand each. How dense the coat looks is
        this against the pixel count, so a bigger picture wants more.
    steps:
        How many steps each particle is followed for.
    color_from_field:
        Colour each strand from the lit height map instead of a grey ramp.
    palette:
        With ``color_from_field``, the ramp the relief is coloured through.
        Without it, the ramp from dark hair to a strand in full sheen.
    frizz:
        How hard each strand is pushed a way of its own, as a fraction of
        the force. At 0 they lie perfectly parallel.
    swirl:
        Degrees the force is turned through. 0 runs particles into the
        field; near 90 sends them round it.
    surface:
        A :class:`Hair` to start from. Use this when you need the knobs the
        signature does not list (``drag``, ``thickness``, ``taper``, …).

    Returns
    -------
    Image
        Three bytes per pixel. Like the globe this follows paths rather than
        sample points, so ``detail`` plays no part.
    """
    settings = surface if surface is not None else Hair()
    settings.seed = seed
    if count is not None:
        settings.count = count
    if steps is not None:
        settings.steps = steps
    if color_from_field:
        settings.color_from_field = True
    if palette is not None:
        settings.palette = palette
        map_surface = settings.surface
        map_surface.palette = palette
        settings.surface = map_surface
    if frizz is not None:
        settings.frizz = frizz
    if swirl is not None:
        settings.swirl = swirl
    dims = _cells(cells)
    pixels = _pair(size)
    return Renderer(Instance(dims, seed)).hair(pixels[0], pixels[1], settings)
