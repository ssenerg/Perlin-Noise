//! Renders a single PNG from command line options.
//!
//! `cargo run --example image --features png -- --mode relief --out noise.png`
//!
//! The `justfile` wraps this, so `just image relief` does the same thing.

use std::io::{Error, ErrorKind, Result};

use perlin_noise::{Globe, Image, Instance, Palette, Relief, Renderer, Shape};

const USAGE: &str = "\
Options:
  --mode <gray|color|channels|relief|globe>  what to draw (default relief)
  --cells <n|down,across>   noise cells per axis (default 8, globe 7,2)
  --detail <n|down,across>  samples per cell, so the image comes out
                    cells*detail+1 pixels on each axis (default 64)
  --seed <n>        lattice seed (default 1)
  --palette <name>  gray, bluered, heat, terrain, or crimson and azure, which
                    are the two built for --mode relief (default crimson)
  --out <path>      where to write the PNG (default noise.png)
  --gpu             sample on the GPU (needs the gpu feature)

For --mode globe, a sphere carved out of a four dimensional field:
  --cells <n|n,depth>  cells across the sphere, then cells along the fourth
                    axis, which the colour and height are read off (7,2)
  --size <n|wide,tall>  image in pixels; --detail plays no part here (1024)
  --fill <f>        how much of the shorter side the sphere fills (0.92)
  --samples <n>     shading samples per pixel along each axis, so 2 shades a
                    pixel four times; 1 leaves the highlights speckled (2)
  --palette <name>  colour the sphere by height through this ramp instead of
                    taking red, green and blue off the fourth axis

Lighting, for --mode relief and --mode globe, defaults in brackets as
relief/globe:
  --shape <name>    smooth, billow or ridged (billow)
  --height <f>      relief: how far the height map is exaggerated; globe: how
                    far the surface rises, as a fraction of its radius, which
                    settles past about 1.0 (0.32/0.9)
  --gamma <f>       height curve; above 1.0 opens the creases into wider,
                    gentler basins (1.2/1.6)
  --occlusion <f>   how dark the low ground goes (0.78/0.7)
  --light <x,y,z>   where the light comes from: x right, y down, z towards the
                    viewer (-0.55,-0.55,0.63)
  --ambient <f>     brightness facing away from the light (0.24/0.3)
  --diffuse <f>     brightness facing the light (0.9/1.2)
  --specular <f>    strength of the glossy highlight (0.45)
  --gloss <f>       tightness of that highlight (80/300)";

/// Cells along the fourth axis of a globe, the one its colour and height are
/// read off. More than one cell puts the four depths further apart, which is
/// what keeps the three colour channels from agreeing with each other.
const GLOBE_DEPTH: usize = 2;

struct Options {
    mode: String,
    /// One value for a square image, or one per axis.
    cells: Vec<usize>,
    detail: Vec<usize>,
    /// Pixels across then down, for modes that do not take their size from
    /// the lattice.
    size: Vec<usize>,
    seed: u64,
    palette: Palette,
    /// Set when `--palette` was asked for, which is what turns a globe over to
    /// colouring by height.
    palette_given: bool,
    relief: Relief,
    fill: f64,
    samples: usize,
    out: String,
    gpu: bool,
}

impl Options {
    /// Defaults for one mode. A sphere carries taller bumps and less
    /// darkening than the flat relief, so each mode starts from its own.
    fn for_mode(mode: &str) -> Self {
        let globe = Globe::default();
        let sphere = mode == "globe";
        Self {
            mode: mode.to_string(),
            cells: if sphere {
                vec![7, GLOBE_DEPTH]
            } else {
                vec![8]
            },
            detail: vec![64],
            size: vec![1024],
            seed: 1,
            palette: Palette::Crimson,
            palette_given: false,
            relief: if sphere {
                globe.surface
            } else {
                Relief::default()
            },
            fill: globe.fill,
            samples: globe.samples,
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
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The mode comes first whatever order it was written in, because it picks
    // the defaults every other flag then overrides.
    let mode = args
        .windows(2)
        .find(|pair| pair[0] == "--mode")
        .map(|pair| pair[1].as_str())
        .unwrap_or("relief");
    let mut options = Options::for_mode(mode);
    let mut args = args.into_iter();

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
        // One value for both axes, or a pair, written `8,16` or `1920x1080`.
        let per_axis = || -> Result<Vec<usize>> {
            let mut axes = Vec::new();
            for part in value.split([',', 'x']) {
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
            "--size" => options.size = per_axis()?,
            "--fill" => options.fill = number()?,
            "--samples" => {
                options.samples = value
                    .parse()
                    .map_err(|_| bad(format!("--samples wants a whole number, got {value}")))?
            }
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
                options.palette_given = true;
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
    // three slices the channels come from. A globe spends three axes on the
    // sphere and reads its height and colour off the fourth.
    let (dims, divisors) = match options.mode.as_str() {
        "channels" => (vec![cells[0], cells[1], 1], vec![detail[0], detail[1], 8]),
        "globe" => {
            // Three axes of the same size for the sphere, then the axis its
            // surface is read off, which one --cells value leaves alone.
            let across = cells[0];
            let depth = options.cells.get(1).copied().unwrap_or(GLOBE_DEPTH);
            (vec![across, across, across, depth], Vec::new())
        }
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
        "globe" => {
            let size = both_axes(&options.size);
            renderer.globe(
                size[0],
                size[1],
                &Globe {
                    surface: options.relief,
                    // Asking for a palette is asking for the sphere to be
                    // coloured by height rather than by the fourth axis.
                    rgb_from_fourth_axis: !options.palette_given,
                    fill: options.fill,
                    samples: options.samples,
                    ..Default::default()
                },
            )
        }
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
