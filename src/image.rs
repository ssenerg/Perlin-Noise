//! Turning a noise field into an image.
//!
//! A [`Renderer`] wraps a two or three dimensional [`Instance`] and samples it
//! on a refined lattice, exactly like [`Instance::table`], so `divisors` means
//! the same thing here: axis `i` ends up with `dims[i] * divisors[i] + 1`
//! samples, and that count becomes a pixel count.
//!
//! Axes map to the image the way a row-major array does, so the **first axis
//! runs down the image** (one row per sample) and the **second runs across it**
//! (one column per sample). For a three dimensional field the third axis is the
//! colour axis.

use std::io::{Error, ErrorKind, Result};

use crate::{Instance, Table};

/// Samples a noise field into pixels.
pub struct Renderer {
    noise: Instance,
    #[cfg(feature = "gpu")]
    gpu: bool,
}

impl Renderer {
    /// Wraps a field of two dimensions (greyscale) or three (colour).
    pub fn new(noise: Instance) -> Result<Self> {
        let n = noise.dims().len();
        if n != 2 && n != 3 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Images need 2 or 3 dimensions, got {n}"),
            ));
        }
        Ok(Self {
            noise,
            #[cfg(feature = "gpu")]
            gpu: false,
        })
    }

    /// Sample on the GPU instead of the CPU.
    ///
    /// Requires the `gpu` feature. The shader computes in 32 bit floats, which
    /// is far finer than the 8 bits a pixel keeps, so the image is the same
    /// either way. Rendering reports an error rather than falling back if no
    /// adapter is available.
    #[cfg(feature = "gpu")]
    pub fn with_gpu(mut self, enabled: bool) -> Self {
        self.gpu = enabled;
        self
    }

    /// The wrapped field, for sampling single points.
    pub fn instance(&self) -> &Instance {
        &self.noise
    }

    /// Gives the wrapped field back.
    pub fn into_instance(self) -> Instance {
        self.noise
    }

    /// Greyscale image of a two dimensional field, one pixel per sample.
    pub fn grayscale(&self, divisors: &[usize]) -> Result<Image> {
        self.expect_dims(2)?;
        let table = self.sample(divisors)?;
        let (height, width) = (table.shape()[0], table.shape()[1]);

        let span = Span::of(table.values());
        let pixels = table
            .values()
            .iter()
            .map(|value| span.byte(*value))
            .collect();

        Ok(Image {
            width,
            height,
            channels: 1,
            pixels,
        })
    }

    /// Colour image of a three dimensional field, taking the red, green and
    /// blue channels from three evenly spaced slices of the third axis.
    ///
    /// The third axis therefore needs at least three samples, i.e.
    /// `dims[2] * divisors[2] + 1 >= 3`. Spreading the slices over the whole
    /// axis leaves the channels largely independent, which reads as strong
    /// colour; [`Renderer::rgb_channels_at`] can place them closer together.
    pub fn rgb_channels(&self, divisors: &[usize]) -> Result<Image> {
        self.expect_dims(3)?;
        let depth = self.color_axis(divisors)?;
        if depth < 3 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "The colour axis has {depth} samples but needs at least 3; raise its divisor"
                ),
            ));
        }
        self.rgb_channels_at(divisors, [0, (depth - 1) / 2, depth - 1])
    }

    /// Colour image of a three dimensional field, taking the red, green and
    /// blue channels from three chosen slices of the third axis.
    ///
    /// Each entry of `slices` indexes the third axis of the refined lattice, so
    /// all three must be below `dims[2] * divisors[2] + 1`. How far apart they
    /// sit decides how much the channels have in common: neighbouring slices
    /// give soft, closely related colours, distant ones give vivid unrelated
    /// channels.
    pub fn rgb_channels_at(&self, divisors: &[usize], slices: [usize; 3]) -> Result<Image> {
        self.expect_dims(3)?;
        let table = self.sample(divisors)?;
        let (height, width, depth) = (table.shape()[0], table.shape()[1], table.shape()[2]);
        for slice in slices {
            if slice >= depth {
                return Err(Error::new(
                    ErrorKind::InvalidInput,
                    format!("Slice {slice} is outside the colour axis, which has {depth} samples"),
                ));
            }
        }

        // One span across all three channels, so a colour cast in the field
        // survives instead of being normalised away channel by channel.
        let span = Span::of(
            slices
                .iter()
                .flat_map(|slice| table.values()[*slice..].iter().step_by(depth)),
        );

        let mut pixels = Vec::with_capacity(width * height * 3);
        for pixel in 0..width * height {
            for slice in slices {
                pixels.push(span.byte(table.values()[pixel * depth + slice]));
            }
        }

        Ok(Image {
            width,
            height,
            channels: 3,
            pixels,
        })
    }

    /// Colour image of a two dimensional field, running each value through
    /// `palette`.
    pub fn colored(&self, divisors: &[usize], palette: Palette) -> Result<Image> {
        self.expect_dims(2)?;
        let table = self.sample(divisors)?;
        let (height, width) = (table.shape()[0], table.shape()[1]);

        let span = Span::of(table.values());
        let mut pixels = Vec::with_capacity(width * height * 3);
        for value in table.values() {
            pixels.extend_from_slice(&palette.color(span.position(*value)));
        }

        Ok(Image {
            width,
            height,
            channels: 3,
            pixels,
        })
    }

    /// Lit image of a two dimensional field, read as a height map so it comes
    /// out looking three dimensional.
    ///
    /// The noise is shaped by [`Relief::shape`], the surface normal is taken
    /// from the slope between neighbouring samples, and each pixel is then lit
    /// and tinted by [`Relief::palette`]. See [`Relief`] for the knobs.
    pub fn relief(&self, divisors: &[usize], relief: &Relief) -> Result<Image> {
        self.expect_dims(2)?;
        let table = self.sample(divisors)?;
        let (height, width) = (table.shape()[0], table.shape()[1]);

        let shaped: Vec<f64> = table
            .values()
            .iter()
            .map(|value| relief.shape.apply(*value))
            .collect();
        let span = Span::of(shaped.iter());
        let heights: Vec<f64> = shaped
            .iter()
            .map(|value| span.position(*value).powf(relief.gamma))
            .collect();

        let light = normalize(relief.light);
        // Halfway between the light and a viewer looking straight on, which is
        // where a glossy surface throws its highlight.
        let half = normalize([light[0], light[1], light[2] + 1.0]);
        // Slopes are measured in noise units rather than pixels, so the relief
        // looks the same however finely the field is sampled.
        let step_down = 1.0 / divisors[0] as f64;
        let step_across = 1.0 / divisors[1] as f64;

        let mut pixels = Vec::with_capacity(width * height * 3);
        for row in 0..height {
            for col in 0..width {
                let at = |row: usize, col: usize| heights[row * width + col];
                let up = row.saturating_sub(1);
                let down = (row + 1).min(height - 1);
                let left = col.saturating_sub(1);
                let right = (col + 1).min(width - 1);
                // Central differences, one sided along the border.
                let slope_down = slope(at(down, col) - at(up, col), down - up, step_down);
                let slope_across = slope(at(row, right) - at(row, left), right - left, step_across);

                let normal = normalize([
                    -relief.height * slope_across,
                    -relief.height * slope_down,
                    1.0,
                ]);
                let lambert = dot(normal, light).max(0.0);
                let highlight = dot(normal, half).max(0.0).powf(relief.gloss);

                // Stand-in for ambient occlusion: the lower the ground, the
                // less light reaches it. This is what darkens the creases, and
                // because it follows the height smoothly the fall from colour
                // to dark is a gradient rather than an edge.
                let height = at(row, col);
                let reach = 1.0 - relief.occlusion * (1.0 - height);

                let base = relief.palette.color(height);
                for channel in base {
                    // Light in linear space, where twice the value really is
                    // twice the light, and re-encode at the end. Shading a
                    // colour byte directly instead crushes everything near the
                    // dark end into an abrupt edge.
                    // The highlight is gated by the diffuse term because a
                    // surface turned away from the light cannot catch a
                    // reflection of it; without that it washes grey.
                    let lit = to_linear(channel) * (relief.ambient + relief.diffuse * lambert)
                        + relief.specular * highlight * lambert;
                    pixels.push(to_srgb(lit * reach));
                }
            }
        }

        Ok(Image {
            width,
            height,
            channels: 3,
            pixels,
        })
    }

    /// Colour image of a three dimensional field, taking one slice of the
    /// third axis and running each value through `palette`.
    ///
    /// `slice` indexes the third axis of the refined lattice, so it must be
    /// below `dims[2] * divisors[2] + 1`.
    pub fn rgb_palette(&self, divisors: &[usize], slice: usize, palette: Palette) -> Result<Image> {
        self.expect_dims(3)?;
        let table = self.sample(divisors)?;
        let (height, width, depth) = (table.shape()[0], table.shape()[1], table.shape()[2]);
        if slice >= depth {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Slice {slice} is outside the colour axis, which has {depth} samples"),
            ));
        }

        let values = || table.values()[slice..].iter().step_by(depth);
        let span = Span::of(values());

        let mut pixels = Vec::with_capacity(width * height * 3);
        for value in values() {
            pixels.extend_from_slice(&palette.color(span.position(*value)));
        }

        Ok(Image {
            width,
            height,
            channels: 3,
            pixels,
        })
    }

    /// Samples along the third axis for `divisors`, without sampling the field.
    fn color_axis(&self, divisors: &[usize]) -> Result<usize> {
        Ok(self.noise.table_shape(divisors)?[2])
    }

    fn expect_dims(&self, wanted: usize) -> Result<()> {
        let n = self.noise.dims().len();
        if n != wanted {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("This image needs a {wanted} dimensional field, got {n}"),
            ));
        }
        Ok(())
    }

    fn sample(&self, divisors: &[usize]) -> Result<Table> {
        #[cfg(feature = "gpu")]
        if self.gpu {
            return self.noise.table_gpu(divisors);
        }
        self.noise.table(divisors)
    }
}

/// The range a set of noise values covers, used to spread them over the full
/// 0..=255 of a channel.
///
/// Perlin values cluster well inside their theoretical `±sqrt(n)` bound, so
/// mapping that bound straight to bytes would waste most of the range and leave
/// a washed out image. Each image is scaled by its own extremes instead; sample
/// [`Instance::table`] directly if you need values comparable across images.
struct Span {
    min: f64,
    max: f64,
}

impl Span {
    fn of<'a>(values: impl IntoIterator<Item = &'a f64>) -> Self {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for value in values {
            min = min.min(*value);
            max = max.max(*value);
        }
        Self { min, max }
    }

    /// Where `value` sits in the range, from 0.0 to 1.0. A flat field has no
    /// range to speak of and lands in the middle.
    fn position(&self, value: f64) -> f64 {
        if self.max > self.min {
            ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        } else {
            0.5
        }
    }

    fn byte(&self, value: f64) -> u8 {
        (self.position(value) * 255.0).round() as u8
    }
}

/// An sRGB colour byte as linear light.
fn to_linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light back to an sRGB colour byte.
fn to_srgb(value: f64) -> u8 {
    let value = value.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

/// Rise over run, with the flat case that a one pixel image would hit.
fn slope(rise: f64, pixels: usize, step: f64) -> f64 {
    if pixels == 0 {
        0.0
    } else {
        rise / (pixels as f64 * step)
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(vector: [f64; 3]) -> [f64; 3] {
    let norm = dot(vector, vector).sqrt();
    if norm > 0.0 {
        [vector[0] / norm, vector[1] / norm, vector[2] / norm]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// How the noise is shaped into a height map before it is lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Height is the noise itself: smooth hills and hollows.
    Smooth,
    /// Height is the distance from zero, which turns the lines where the noise
    /// changes sign into sharp creases between rounded blobs.
    Billow,
    /// Billow upside down: sharp ridges with wide smooth basins between them.
    Ridged,
}

impl Shape {
    fn apply(&self, value: f64) -> f64 {
        match self {
            Shape::Smooth => value,
            Shape::Billow => value.abs(),
            Shape::Ridged => -value.abs(),
        }
    }
}

/// Lighting and colour for [`Renderer::relief`].
///
/// [`Relief::default`] is the glossy liquid look: billowing blobs lit from the
/// top left. The fields are tuned by eye, so treat the defaults as a starting
/// point rather than physics.
#[derive(Clone, Copy, Debug)]
pub struct Relief {
    /// How the noise becomes a height map.
    pub shape: Shape,
    /// Colour ramp applied across the height range.
    pub palette: Palette,
    /// Where the light comes from, in image space: x to the right, y down the
    /// image, z out towards the viewer. Length does not matter.
    pub light: [f64; 3],
    /// How far the height map is exaggerated before slopes are measured.
    /// Higher means steeper sides and harsher shading.
    pub height: f64,
    /// Curve applied to the height map, which both the shading and the palette
    /// then read. Below 1.0 flattens the tops into plateaus and steepens the
    /// creases; above 1.0 opens the creases into wide smooth basins, which is
    /// what keeps the fall from colour to dark gradual.
    pub gamma: f64,
    /// How much the low ground is darkened for sitting in its own shadow, 0.0
    /// to 1.0. This is what makes the creases dark, and it follows the height
    /// smoothly so the falloff stays gradual.
    pub occlusion: f64,
    /// Brightness of surfaces facing away from the light, 0.0 to 1.0.
    pub ambient: f64,
    /// Brightness of surfaces facing the light, 0.0 to 1.0.
    pub diffuse: f64,
    /// Strength of the glossy highlight, 0.0 to 1.0.
    pub specular: f64,
    /// Tightness of that highlight. Higher is smaller and sharper.
    pub gloss: f64,
}

impl Default for Relief {
    fn default() -> Self {
        Self {
            shape: Shape::Billow,
            palette: Palette::Crimson,
            light: [-0.55, -0.55, 0.63],
            height: 0.32,
            gamma: 1.2,
            occlusion: 0.78,
            ambient: 0.24,
            diffuse: 0.9,
            specular: 0.45,
            gloss: 80.0,
        }
    }
}

/// Maps a noise value to a colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    /// Black through to white.
    Gray,
    /// Blue through white to red, for telling low from high at a glance.
    BlueRed,
    /// Black through red and orange to white.
    Heat,
    /// Deep red to pink, for a glossy liquid look under [`Renderer::relief`].
    Crimson,
    /// Navy to bright blue: [`Palette::Crimson`] in blue.
    Azure,
    /// Deep water, shallow water, sand, grass, rock, snow.
    Terrain,
}

impl Palette {
    /// Evenly spaced colours to interpolate between.
    fn stops(&self) -> &'static [[u8; 3]] {
        match self {
            Palette::Gray => &[[0, 0, 0], [255, 255, 255]],
            Palette::BlueRed => &[[33, 102, 172], [247, 247, 247], [178, 24, 43]],
            Palette::Heat => &[
                [0, 0, 0],
                [128, 0, 0],
                [255, 80, 0],
                [255, 200, 0],
                [255, 255, 255],
            ],
            // Deep red to pink with no black at all: in a relief the dark
            // belongs to the shading, and a ramp that dives into black instead
            // draws a hard edge wherever the surface dips.
            Palette::Crimson => &[
                [92, 13, 33],
                [122, 16, 40],
                [148, 18, 46],
                [170, 21, 54],
                [188, 25, 62],
                [202, 31, 72],
                [214, 40, 86],
                [226, 58, 104],
            ],
            // The same climb in blue, likewise with no black in it.
            Palette::Azure => &[
                [12, 30, 88],
                [15, 38, 118],
                [18, 46, 146],
                [22, 56, 172],
                [28, 68, 192],
                [38, 84, 208],
                [54, 104, 220],
                [78, 132, 234],
            ],
            Palette::Terrain => &[
                [12, 44, 92],
                [40, 110, 170],
                [214, 199, 138],
                [92, 148, 74],
                [120, 110, 100],
                [250, 250, 250],
            ],
        }
    }

    /// Colour at `position`, which is clamped to 0.0..=1.0.
    pub fn color(&self, position: f64) -> [u8; 3] {
        let stops = self.stops();
        let scaled = position.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
        let lower = (scaled.floor() as usize).min(stops.len() - 1);
        let upper = (lower + 1).min(stops.len() - 1);
        let blend = scaled - lower as f64;

        let mut color = [0u8; 3];
        for channel in 0..3 {
            let from = f64::from(stops[lower][channel]);
            let to = f64::from(stops[upper][channel]);
            color[channel] = (from + (to - from) * blend).round() as u8;
        }
        color
    }
}

/// An 8 bit image, rows top to bottom, `channels` bytes per pixel.
pub struct Image {
    width: usize,
    height: usize,
    channels: usize,
    pixels: Vec<u8>,
}

impl Image {
    /// Pixels across, one per sample of the second axis.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Pixels down, one per sample of the first axis.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Bytes per pixel: 1 for greyscale, 3 for colour.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// The whole buffer, row-major.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Consumes the image and returns its buffer.
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    /// One pixel's bytes, or `None` outside the image.
    pub fn pixel(&self, x: usize, y: usize) -> Option<&[u8]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = (y * self.width + x) * self.channels;
        Some(&self.pixels[start..start + self.channels])
    }

    /// Writes the image as a PNG.
    ///
    /// Requires the `png` feature.
    #[cfg(feature = "png")]
    pub fn write_png(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut encoder = png::Encoder::new(file, self.width as u32, self.height as u32);
        encoder.set_color(if self.channels == 1 {
            png::ColorType::Grayscale
        } else {
            png::ColorType::Rgb
        });
        encoder.set_depth(png::BitDepth::Eight);

        let mut writer = encoder.write_header().map_err(Error::other)?;
        writer
            .write_image_data(&self.pixels)
            .map_err(Error::other)?;
        writer.finish().map_err(Error::other)
    }
}
