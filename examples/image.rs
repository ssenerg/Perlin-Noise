//! Renders a single PNG from command line options.
//!
//! `cargo run --example image --features png -- --mode relief --out noise.png`
//!
//! The `justfile` wraps this, so `just image relief` does the same thing.

use std::io::{Error, ErrorKind, Result};

use perlin_noise::{Image, Instance, Palette, Relief, Renderer, Shape};

const USAGE: &str = "\
Options:
  --mode <gray|color|channels|relief>  what to draw (default relief)
  --cells <n|down,across>   noise cells per axis (default 8)
  --detail <n|down,across>  samples per cell, so the image comes out
                    cells*detail+1 pixels on each axis (default 64)
  --seed <n>        lattice seed (default 1)
  --palette <name>  gray, bluered, heat, terrain, or crimson and azure, which
                    are the two built for --mode relief (default crimson)
  --out <path>      where to write the PNG (default noise.png)
  --gpu             sample on the GPU (needs the gpu feature)

For --mode relief, with the defaults in brackets:
  --shape <name>    smooth, billow or ridged (billow)
  --height <f>      how far the height map is exaggerated (0.32)
  --gamma <f>       height curve; above 1.0 opens the creases into wider,
                    gentler basins (1.2)
  --occlusion <f>   how dark the low ground goes (0.78)
  --light <x,y,z>   where the light comes from: x right, y down, z towards the
                    viewer (-0.55,-0.55,0.63)
  --ambient <f>     brightness facing away from the light (0.24)
  --diffuse <f>     brightness facing the light (0.9)
  --specular <f>    strength of the glossy highlight (0.45)
  --gloss <f>       tightness of that highlight (80)";

struct Options {
    mode: String,
    /// One value for a square image, or one per axis.
    cells: Vec<usize>,
    detail: Vec<usize>,
    seed: u64,
    palette: Palette,
    relief: Relief,
    out: String,
    gpu: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            mode: "relief".into(),
            cells: vec![8],
            detail: vec![64],
            seed: 1,
            palette: Palette::Crimson,
            relief: Relief::default(),
            out: "noise.png".into(),
            gpu: false,
        }
    }
}

fn bad(message: impl Into<String>) -> Error {
    Error::new(
        ErrorKind::InvalidInput,
        format!("{}\n\n{USAGE}", message.into()),
    )
}

fn parse() -> Result<Options> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);

    while let Some(flag) = args.next() {
        // Every flag but --gpu takes a value.
        if flag == "--gpu" {
            options.gpu = true;
            continue;
        }
        if flag == "--help" || flag == "-h" {
            println!("{USAGE}");
            std::process::exit(0);
        }
        let value = args
            .next()
            .ok_or_else(|| bad(format!("{flag} needs a value")))?;
        // One value for both axes, or `down,across`.
        let per_axis = || -> Result<Vec<usize>> {
            let mut axes = Vec::new();
            for part in value.split(',') {
                axes.push(
                    part.trim()
                        .parse()
                        .map_err(|_| bad(format!("{flag} wants whole numbers, got {part}")))?,
                );
            }
            if axes.is_empty() || axes.len() > 2 {
                return Err(bad(format!("{flag} wants one value or two, got {value}")));
            }
            Ok(axes)
        };
        let number = || -> Result<f64> {
            value
                .parse()
                .map_err(|_| bad(format!("{flag} wants a number, got {value}")))
        };

        match flag.as_str() {
            "--mode" => options.mode = value.clone(),
            "--cells" => options.cells = per_axis()?,
            "--detail" => options.detail = per_axis()?,
            "--seed" => {
                options.seed = value
                    .parse()
                    .map_err(|_| bad(format!("--seed wants a whole number, got {value}")))?
            }
            "--out" => options.out = value.clone(),
            "--height" => options.relief.height = number()?,
            "--gamma" => options.relief.gamma = number()?,
            "--occlusion" => options.relief.occlusion = number()?,
            "--ambient" => options.relief.ambient = number()?,
            "--diffuse" => options.relief.diffuse = number()?,
            "--specular" => options.relief.specular = number()?,
            "--gloss" => options.relief.gloss = number()?,
            "--light" => {
                let parts: Vec<&str> = value.split(',').collect();
                let [x, y, z] = parts.as_slice() else {
                    return Err(bad(format!("--light wants x,y,z, got {value}")));
                };
                for (part, into) in [x, y, z].iter().zip(&mut options.relief.light) {
                    *into = part
                        .trim()
                        .parse()
                        .map_err(|_| bad(format!("--light wants numbers, got {part}")))?;
                }
            }
            "--palette" => {
                options.palette = match value.as_str() {
                    "gray" | "grey" => Palette::Gray,
                    "bluered" => Palette::BlueRed,
                    "heat" => Palette::Heat,
                    "crimson" => Palette::Crimson,
                    "azure" => Palette::Azure,
                    "terrain" => Palette::Terrain,
                    other => return Err(bad(format!("unknown palette {other}"))),
                };
                options.relief.palette = options.palette;
            }
            "--shape" => {
                options.relief.shape = match value.as_str() {
                    "smooth" => Shape::Smooth,
                    "billow" => Shape::Billow,
                    "ridged" => Shape::Ridged,
                    other => return Err(bad(format!("unknown shape {other}"))),
                }
            }
            other => return Err(bad(format!("unknown option {other}"))),
        }
    }

    Ok(options)
}

/// A single value covers both axes; two are taken as down then across.
fn both_axes(axes: &[usize]) -> Vec<usize> {
    match axes {
        [only] => vec![*only, *only],
        rest => rest.to_vec(),
    }
}

fn draw(options: &Options) -> Result<Image> {
    let cells = both_axes(&options.cells);
    let detail = both_axes(&options.detail);

    // The colour axis of a channel image only needs to be deep enough for the
    // three slices the channels come from.
    let (dims, divisors) = match options.mode.as_str() {
        "channels" => (vec![cells[0], cells[1], 1], vec![detail[0], detail[1], 8]),
        _ => (cells, detail),
    };
    let renderer = Renderer::new(Instance::new(dims, options.seed)?)?;

    #[cfg(feature = "gpu")]
    let renderer = renderer.with_gpu(options.gpu);
    #[cfg(not(feature = "gpu"))]
    if options.gpu {
        return Err(bad("--gpu needs the crate's gpu feature"));
    }

    match options.mode.as_str() {
        "gray" | "grey" => renderer.grayscale(&divisors),
        "color" | "colour" => renderer.colored(&divisors, options.palette),
        "channels" => renderer.rgb_channels(&divisors),
        "relief" => renderer.relief(&divisors, &options.relief),
        other => Err(bad(format!("unknown mode {other}"))),
    }
}

fn main() -> Result<()> {
    let options = parse()?;
    let image = draw(&options)?;

    if let Some(parent) = std::path::Path::new(&options.out).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    image.write_png(&options.out)?;

    println!(
        "{}: {}x{} {}",
        options.out,
        image.width(),
        image.height(),
        options.mode
    );
    Ok(())
}
