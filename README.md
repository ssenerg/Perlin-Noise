# Perlin Noise

A seeded Perlin noise field in any number of dimensions, with a renderer that
turns it into an image. The same lattice can be sampled as a number, drawn as
a height map, wrapped onto a sphere, or read as a force that combs a coat of
hair.

The Python package is [`perlin-noise-rs`](https://pypi.org/project/perlin-noise-rs/)
on PyPI (`perlin-noise` is already taken). The import is `perlin_noise`. The
Rust crate is this repository.

<p align="center">
  <img src="https://raw.githubusercontent.com/ssenerg/Perlin-Noise/master/docs/showcase.png" width="640" alt="Glossy red relief rendered from a 2D noise field">
</p>

<p align="center">
  <em>A two-dimensional field, read as a height map and lit.</em>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/ssenerg/Perlin-Noise/master/docs/globe.png" width="280" alt="A lit sphere whose bumps and colours come from a 4D noise field">
  <img src="https://raw.githubusercontent.com/ssenerg/Perlin-Noise/master/docs/hair.png" width="280" alt="A coat of hair combed by a 2D noise field">
  <img src="https://raw.githubusercontent.com/ssenerg/Perlin-Noise/master/docs/hair-color.png" width="280" alt="The same coat painted with the field's relief">
</p>

<p align="center">
  <em>A sphere carved out of four dimensions; a coat of hair; the same coat coloured by the field's relief.</em>
</p>

## What it does

- **Sample** a field at a point, or on a finer lattice. `dims` is the number of
  cells per axis; the same `seed` always rebuilds the same field.
- **Draw** two-dimensional noise as greyscale, through a colour ramp, or as
  three slices of a 3D field for red, green and blue.
- **Light** a height map (`relief`). The default is the glossy liquid look in
  the image above.
- **Carve a sphere** (`globe`) out of a four-dimensional field, so there is no
  seam and no stretching at the poles.
- **Grow hair** (`hair`) by dropping particles into the field and drawing the
  paths they take. Optionally colour the coat with the same field's relief.

Every public type and helper has a docstring. After install,
`help(perlin_noise)` and `help(perlin_noise.relief)` are the reference;
stubs ship with the wheel so editors see them too.

## Installation

```sh
pip install perlin-noise-rs
```

Requires Python 3.10 or later. Pre-built wheels cover the usual Linux, macOS
and Windows platforms.

From Rust, add the crate and turn on `png` if you want to write images:

```toml
perlin-noise = { git = "https://github.com/ssenerg/Perlin-Noise", features = ["png"] }
```

## Quick start

```python
from perlin_noise import Instance, Palette, globe, hair, relief

noise = Instance([8, 8], seed=7)
value = noise.noise([3.5, 2.0])

relief(seed=7, cells=12).write_png("relief.png")
globe(seed=3, palette=Palette.CRIMSON).write_png("globe.png")
hair(seed=7, size=1440, color_from_field=True).write_png("hair.png")
```

`relief`, `globe`, `hair`, `grayscale`, `colored` and `channels` build the
field and render in one call. `Instance` and `Renderer` are there when you
want the pieces.

```rust
use perlin_noise::{Instance, Relief, Renderer};

let noise = Instance::new(vec![8, 8], 7)?;
let value = noise.noise(&[3.5, 2.0])?;

Renderer::new(noise)?
    .relief(&[64, 64], &Relief::default())?
    .write_png("relief.png")?;
```

## Examples

Longer scripts live in [`examples/`](examples/):

| File | What it shows |
| --- | --- |
| [`examples/noise.py`](examples/noise.py) | Sampling a field: a point, a table, the force |
| [`examples/render.py`](examples/render.py) | Every picture mode |
| [`examples/hair.py`](examples/hair.py) | Hair helper, settings, and `Renderer.hair` |
| [`examples/render.rs`](examples/render.rs) | The same gallery from Rust |

## License

MIT. See [LICENSE](LICENSE).
