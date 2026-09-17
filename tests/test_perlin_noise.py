"""Python API checks. Run after `just python` so the extension is importable."""

from perlin_noise import (
    Hair,
    Instance,
    Palette,
    Renderer,
    Shape,
    channels,
    grayscale,
    hair,
    relief,
)


def test_instance_docs_and_sampling():
    assert Instance.__doc__ and "cells" in Instance.__doc__
    assert Instance.noise.__doc__ and "point" in Instance.noise.__doc__

    noise = Instance([3, 1, 2], seed=42)
    assert noise.dims == [3, 1, 2]
    assert noise.shape == [4, 2, 3]
    assert noise.noise([1.5, 0.25, 2.0]) ** 2 <= 3
    table = noise.table([2, 1, 4])
    assert table.shape == [7, 2, 9]
    assert len(table) == 7 * 2 * 9


def test_helpers_render():
    gray = grayscale(cells=4, seed=3, detail=1)
    assert gray.width == 5
    assert gray.height == 5
    assert gray.channels == 1
    assert gray.pixel(0, 0) == bytes([128])

    picture = relief(cells=4, seed=3, detail=8, shape=Shape.BILLOW, palette=Palette.CRIMSON)
    assert picture.channels == 3
    assert picture.width == 33

    rgb = channels(cells=4, seed=3, detail=4, depth=4)
    assert rgb.channels == 3


def test_hair_and_renderer():
    image = hair(cells=3, seed=1, size=48, count=80, steps=20)
    assert image.width == 48
    assert image.channels == 3

    tinted = hair(
        cells=3,
        seed=1,
        size=48,
        count=80,
        steps=20,
        color_from_field=True,
        palette=Palette.AZURE,
    )
    assert tinted.pixels != image.pixels

    renderer = Renderer(Instance([4, 4], 1))
    assert "Renderer" in repr(renderer)
    assert renderer.grayscale([2, 2]).channels == 1


def test_hair_settings_round_trip():
    settings = Hair(count=100, color_from_field=True)
    assert settings.count == 100
    assert settings.color_from_field is True
    settings.frizz = 0.2
    assert settings.frizz == 0.2
