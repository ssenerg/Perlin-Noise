//! Writes one image per rendering mode.
//!
//! `cargo run --example render --features png -- [output directory]`

use std::path::PathBuf;

use perlin_noise::{Instance, Palette, Relief, Renderer, Shape};

fn main() -> std::io::Result<()> {
    let out: PathBuf = std::env::args().nth(1).unwrap_or_else(|| ".".into()).into();
    let seed = 20260915;

    // 8x8 cells sampled 64 times per cell, so a 513x513 image.
    let flat = Renderer::new(Instance::new(vec![8, 8], seed)?)?;
    let gray = flat.grayscale(&[64, 64])?;
    gray.write_png(out.join("noise-gray.png"))?;
    println!(
        "noise-gray.png: {}x{} greyscale",
        gray.width(),
        gray.height()
    );

    // One cell is enough for the colour axis: it only has to be deep enough
    // for the slices the channels come from.
    let deep = Renderer::new(Instance::new(vec![8, 8, 1], seed)?)?;

    // Slices spread over the whole axis, so the channels share little.
    let vivid = deep.rgb_channels(&[64, 64, 8])?;
    vivid.write_png(out.join("noise-channels.png"))?;
    println!(
        "noise-channels.png: {}x{} from 3 spread out slices",
        vivid.width(),
        vivid.height()
    );

    // Neighbouring slices instead, an eighth of a cell apart, for colours that
    // drift rather than clash.
    let soft = deep.rgb_channels_at(&[64, 64, 8], [0, 1, 2])?;
    soft.write_png(out.join("noise-channels-soft.png"))?;
    println!(
        "noise-channels-soft.png: {}x{} from 3 neighbouring slices",
        soft.width(),
        soft.height()
    );

    // One slice of the colour axis, coloured by a ramp rather than by channel.
    let sliced = deep.rgb_palette(&[64, 64, 1], 0, Palette::BlueRed)?;
    sliced.write_png(out.join("noise-bluered.png"))?;
    println!(
        "noise-bluered.png: {}x{} from one slice through BlueRed",
        sliced.width(),
        sliced.height()
    );

    // Ramps straight over the flat two dimensional field.
    for (palette, name) in [
        (Palette::Heat, "noise-heat.png"),
        (Palette::Terrain, "noise-terrain.png"),
    ] {
        let image = flat.colored(&[64, 64], palette)?;
        image.write_png(out.join(name))?;
        println!(
            "{name}: {}x{} through {palette:?}",
            image.width(),
            image.height()
        );
    }

    // The same field read as a height map and lit, once per shape.
    for (shape, name) in [
        (Shape::Billow, "noise-relief-billow.png"),
        (Shape::Ridged, "noise-relief-ridged.png"),
        (Shape::Smooth, "noise-relief-smooth.png"),
    ] {
        let image = flat.relief(
            &[64, 64],
            &Relief {
                shape,
                ..Default::default()
            },
        )?;
        image.write_png(out.join(name))?;
        println!(
            "{name}: {}x{} lit as {shape:?}",
            image.width(),
            image.height()
        );
    }

    Ok(())
}
