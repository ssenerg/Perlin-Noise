//! Turning a noise field into an image.
//!
//! A [`Renderer`] wraps an [`Instance`] and samples it on a refined lattice,
//! exactly like [`Instance::table`], so `divisors` means the same thing here:
//! axis `i` ends up with `dims[i] * divisors[i] + 1` samples, and that count
//! becomes a pixel count.
//!
//! Axes map to the image the way a row-major array does, so the **first axis
//! runs down the image** (one row per sample) and the **second runs across it**
//! (one column per sample). For a three dimensional field the third axis is the
//! colour axis.
//!
//! [`Renderer::globe`] is the exception: it reads a four dimensional field
//! along the surface of a sphere instead of along a lattice, so it takes a
//! size in pixels and ignores `divisors`.

use std::io::{Error, ErrorKind, Result};

use crate::{Instance, PARALLEL_THRESHOLD, Table};

/// Samples a noise field into pixels.
pub struct Renderer {
    pub(crate) noise: Instance,
    #[cfg(feature = "gpu")]
    gpu: bool,
}

impl Renderer {
    /// Wraps a field of two dimensions (greyscale or relief), three (colour) or
    /// four ([`Renderer::globe`]).
    pub fn new(noise: Instance) -> Result<Self> {
        let n = noise.dims().len();
        if !(2..=4).contains(&n) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Images need 2 to 4 dimensions, got {n}"),
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

        let span = Span::of(table.values().iter().copied());
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
                .flat_map(|slice| table.values()[*slice..].iter().step_by(depth).copied()),
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

        let span = Span::of(table.values().iter().copied());
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
        let span = Span::of(shaped.iter().copied());
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
        let span = Span::of(values().copied());

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

    /// A sphere carved out of a four dimensional field and projected onto a
    /// `width` by `height` image.
    ///
    /// Three of the axes hold the sphere itself: the unit sphere is inscribed
    /// in the lattice box, so a point's direction from the centre *is* its
    /// position in the field. That leaves the surface seamless, with none of
    /// the stretching at the poles a flat texture wrapped around a ball has.
    ///
    /// The fourth axis is sampled at four evenly spaced depths. The first is
    /// the point's distance from the centre, the other three are its red,
    /// green and blue, so neighbouring depths keep colour and height gently
    /// related instead of independent.
    ///
    /// Height tilts the surface and shades it, but does not move the outline,
    /// which stays a circle. Unlike the other modes this one samples points
    /// along a surface rather than a lattice, so it has no GPU path and
    /// `divisors` play no part: `dims` alone set the feature size, in cells
    /// across the sphere's diameter.
    pub fn globe(&self, width: usize, height: usize, globe: &Globe) -> Result<Image> {
        self.expect_dims(4)?;
        if width == 0 || height == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "An image needs a positive width and height",
            ));
        }
        if !(globe.fill > 0.0 && globe.fill <= 1.0) {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("Fill has to be above 0 and at most 1, got {}", globe.fill),
            ));
        }

        if globe.samples == 0 || globe.samples > MAX_SAMPLES {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!(
                    "Samples has to be between 1 and {MAX_SAMPLES}, got {}",
                    globe.samples
                ),
            ));
        }

        let centre = [width as f64 / 2.0, height as f64 / 2.0];
        let radius = globe.fill * width.min(height) as f64 / 2.0;
        let material = &globe.surface;

        // Four evenly spaced depths of the fourth axis, ends included.
        let axis = self.noise.dims()[3] as f64;
        let depths = [0.0, axis / 3.0, axis * 2.0 / 3.0, axis];

        // A look over the whole sphere first, because the spans that turn
        // noise into height and colour are measured over all of it.
        let mut surface = vec![Surface::default(); width * height];
        in_parallel(&mut surface, PARALLEL_THRESHOLD, |index, point| {
            let at = [(index % width) as f64 + 0.5, (index / width) as f64 + 0.5];
            let Some((direction, cover)) = look(at, 1.0, centre, radius) else {
                return;
            };
            point.cover = cover as f32;
            point.height = material.shape.apply(self.on_sphere(direction, depths[0])) as f32;
            for (channel, depth) in point.color.iter_mut().zip(&depths[1..]) {
                *channel = self.on_sphere(direction, *depth) as f32;
            }
        });

        let seen = |point: &&Surface| point.cover > 0.0;
        let lit = Lit {
            renderer: self,
            globe,
            depths,
            heights: Span::of(surface.iter().filter(seen).map(|point| point.height as f64)),
            // One span across all three channels, so a colour cast in the
            // field survives instead of being normalised away channel by
            // channel.
            colors: Span::of(
                surface
                    .iter()
                    .filter(seen)
                    .flat_map(|point| point.color)
                    .map(f64::from),
            ),
            light: normalize(material.light),
            radius,
        };

        // Shading is sampled several times per pixel and averaged. One sample
        // is not enough: a tight highlight on a steep slope is finer than a
        // pixel, and point sampling it leaves the creases speckled.
        let grid = globe.samples;
        let span = 1.0 / grid as f64;
        let mut pixels = vec![[0u8; 3]; width * height];
        in_parallel(&mut pixels, PARALLEL_THRESHOLD, |index, pixel| {
            let corner = [(index % width) as f64, (index / width) as f64];
            let mut light = [0.0; 3];
            let mut covered = 0.0;
            for down in 0..grid {
                for across in 0..grid {
                    let at = [
                        corner[0] + (across as f64 + 0.5) * span,
                        corner[1] + (down as f64 + 0.5) * span,
                    ];
                    let Some((direction, cover)) = look(at, span, centre, radius) else {
                        continue;
                    };
                    for (total, value) in light.iter_mut().zip(lit.point(direction)) {
                        *total += value * cover;
                    }
                    covered += cover;
                }
            }

            let count = (grid * grid) as f64;
            for ((slot, total), behind) in pixel.iter_mut().zip(light).zip(globe.background) {
                // Averaged in linear light, and what the sphere left uncovered
                // is filled in with the background.
                *slot = to_srgb(total / count + to_linear(behind) * (1.0 - covered / count));
            }
        });

        Ok(Image {
            width,
            height,
            channels: 3,
            pixels: pixels.into_iter().flatten().collect(),
        })
    }

    /// Noise at a point on the sphere's surface, at one depth of the fourth
    /// axis.
    fn on_sphere(&self, direction: [f64; 3], depth: f64) -> f64 {
        let dims = self.noise.dims();
        let mut point = [0.0; 4];
        for (axis, coord) in point[..3].iter_mut().enumerate() {
            // The unit sphere inscribed in the lattice box.
            *coord = (direction[axis] + 1.0) * 0.5 * dims[axis] as f64;
        }
        point[3] = depth;
        self.noise.sample(&point)
    }

    /// Samples along the third axis for `divisors`, without sampling the field.
    fn color_axis(&self, divisors: &[usize]) -> Result<usize> {
        Ok(self.noise.table_shape(divisors)?[2])
    }

    pub(crate) fn expect_dims(&self, wanted: usize) -> Result<()> {
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
pub(crate) struct Span {
    min: f64,
    max: f64,
}

impl Span {
    pub(crate) fn of(values: impl IntoIterator<Item = f64>) -> Self {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for value in values {
            min = min.min(value);
            max = max.max(value);
        }
        Self { min, max }
    }

    /// Where `value` sits in the range, from 0.0 to 1.0. A flat field has no
    /// range to speak of and lands in the middle.
    pub(crate) fn position(&self, value: f64) -> f64 {
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
pub(crate) fn to_linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light back to an sRGB colour byte.
pub(crate) fn to_srgb(value: f64) -> u8 {
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

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Two unit vectors at right angles to `direction` and to each other.
fn tangents(direction: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    // Any vector that is not lined up with the direction will do to start.
    let aside = if direction[1].abs() < 0.9 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let across = normalize(cross(aside, direction));
    (across, cross(direction, across))
}

/// Where a point `span` pixels wide, sitting at `at` in the image, meets a
/// sphere of `radius` pixels centred on `centre`: the direction from the
/// sphere's centre to that point, and how much of the point the sphere covers.
/// `None` when it misses.
fn look(at: [f64; 2], span: f64, centre: [f64; 2], radius: f64) -> Option<([f64; 3], f64)> {
    let x = (at[0] - centre[0]) / radius;
    let y = (at[1] - centre[1]) / radius;

    // Coverage fades over the last half width either side of the rim, which is
    // what keeps the outline from going jagged.
    let reach = (x * x + y * y).sqrt();
    let cover = ((1.0 - reach) * radius / span + 0.5).min(1.0);
    if cover <= 0.0 {
        return None;
    }

    // Straight-on projection, so the viewer looks down the z axis and the near
    // face of the sphere is the half with z above 0.
    let z = (1.0 - reach * reach).max(0.0).sqrt();
    Some((normalize([x, y, z]), cover))
}

/// Runs `each` over every item together with its index, split across threads
/// once there are at least `least` of them. How much work one item is worth
/// differs by caller, which is why the threshold comes in rather than being
/// fixed: a pixel is cheap, a whole strand of hair is not.
pub(crate) fn in_parallel<T: Send>(
    items: &mut [T],
    least: usize,
    each: impl Fn(usize, &mut T) + Send + Sync,
) {
    if items.len() < least {
        for (index, item) in items.iter_mut().enumerate() {
            each(index, item);
        }
        return;
    }

    let threads = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    let chunk = items.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (index, part) in items.chunks_mut(chunk).enumerate() {
            let each = &each;
            let start = index * chunk;
            scope.spawn(move || {
                for (offset, item) in part.iter_mut().enumerate() {
                    each(start + offset, item);
                }
            });
        }
    });
}

/// What one pixel of a globe sees, gathered before the spans are known.
///
/// Held in `f32` because a whole frame of these is kept at once and the values
/// only ever become bytes.
#[derive(Clone, Copy, Default)]
struct Surface {
    /// How much of the pixel the sphere covers; 0.0 means it missed.
    cover: f32,
    /// Shaped noise standing for the distance from the centre.
    height: f32,
    /// The three colour channels off the fourth axis.
    color: [f32; 3],
}

/// How far towards the rim a sphere's surface is still measured a pixel at a
/// time. Past this the surface is turned so far from the viewer that a pixel
/// covers an unbounded stretch of it, and the slope would be read over half
/// the sphere.
const RIM: f64 = 0.2;

/// Most shading samples a pixel of a globe is allowed along each axis.
pub const MAX_SAMPLES: usize = 8;

/// One globe's worth of shading, held together so a single surface point can
/// be lit on its own.
struct Lit<'a> {
    renderer: &'a Renderer,
    globe: &'a Globe,
    /// Depths of the fourth axis: height, then red, green and blue.
    depths: [f64; 4],
    heights: Span,
    colors: Span,
    light: [f64; 3],
    /// Radius of the sphere in pixels, which sets how wide a stretch of
    /// surface one pixel covers.
    radius: f64,
}

impl Lit<'_> {
    /// The linear light coming off the point of the surface lying in
    /// `direction` from the centre.
    fn point(&self, direction: [f64; 3]) -> [f64; 3] {
        let material = &self.globe.surface;
        let height = self.rise(self.renderer.on_sphere(direction, self.depths[0]));

        // Slope of the surface along two directions tangent to the sphere,
        // measured a pixel either side. Reading it over the width of a pixel
        // rather than at a point is what stops a crease in the height from
        // turning over inside one pixel; near the rim a pixel covers a longer
        // stretch, because the surface is turned almost edge on.
        let footprint = 1.0 / (self.radius * direction[2].max(RIM));
        let (across, down) = tangents(direction);
        let slope = |tangent: [f64; 3]| {
            let step = |sign: f64| {
                let offset = normalize([
                    direction[0] + sign * footprint * tangent[0],
                    direction[1] + sign * footprint * tangent[1],
                    direction[2] + sign * footprint * tangent[2],
                ]);
                self.rise(self.renderer.on_sphere(offset, self.depths[0]))
            };
            (step(1.0) - step(-1.0)) / (2.0 * footprint)
        };
        let (slope_across, slope_down) = (slope(across), slope(down));

        // Normal of a surface sitting at that height: the outward direction,
        // tipped away from the tangent the height climbs along.
        let lifted = 1.0 + material.height * height;
        let tilt = material.height / lifted;
        let normal = normalize([
            direction[0] - tilt * (slope_across * across[0] + slope_down * down[0]),
            direction[1] - tilt * (slope_across * across[1] + slope_down * down[1]),
            direction[2] - tilt * (slope_across * across[2] + slope_down * down[2]),
        ]);

        let lambert = dot(normal, self.light).max(0.0);
        // Halfway between the light and a viewer looking straight on, which is
        // where a glossy surface throws its highlight.
        let half = normalize([self.light[0], self.light[1], self.light[2] + 1.0]);
        let highlight = dot(normal, half).max(0.0).powf(material.gloss);
        // Stand-in for ambient occlusion: the lower the ground, the less light
        // reaches it.
        let shadowed = 1.0 - material.occlusion * (1.0 - height);

        let base = if self.globe.rgb_from_fourth_axis {
            let mut channels = [0; 3];
            for (channel, depth) in channels.iter_mut().zip(&self.depths[1..]) {
                *channel = self.colors.byte(self.renderer.on_sphere(direction, *depth));
            }
            channels
        } else {
            material.palette.color(height)
        };

        let mut light = [0.0; 3];
        for (out, value) in light.iter_mut().zip(base) {
            // Lit in linear space, like `relief`, where twice the value really
            // is twice the light. The highlight is gated by the diffuse term
            // because a surface turned away from the light cannot catch a
            // reflection of it, and by the occlusion twice over: a hollow deep
            // enough to sit in its own shadow has no clear line to the light
            // to reflect, and a bright seam down every crease is what gives a
            // dark hollow a grey cast.
            *out = (to_linear(value) * (material.ambient + material.diffuse * lambert)
                + material.specular * highlight * lambert * shadowed * shadowed)
                * shadowed;
        }
        light
    }

    /// The run from raw noise to a height between 0.0 and 1.0.
    fn rise(&self, value: f64) -> f64 {
        self.heights
            .position(self.globe.surface.shape.apply(value))
            .powf(self.globe.surface.gamma)
    }
}

/// Lighting and colour for [`Renderer::globe`].
#[derive(Clone, Copy, Debug)]
pub struct Globe {
    /// Height shaping, colour ramp and lighting, as in [`Renderer::relief`].
    /// [`Relief::height`] is read here as how far the surface rises and falls,
    /// as a fraction of the sphere's radius. Past about 1.0 it stops buying
    /// much: taller bumps also swell the sphere they sit on, so the shape
    /// grows with itself and the slopes settle.
    pub surface: Relief,
    /// Take red, green and blue from three depths of the fourth axis. When
    /// this is off the colour comes from the height through
    /// [`Relief::palette`] instead.
    pub rgb_from_fourth_axis: bool,
    /// How much of the shorter side of the image the sphere fills, above 0.0
    /// and up to 1.0.
    pub fill: f64,
    /// Colour behind the sphere.
    pub background: [u8; 3],
    /// Shading samples per pixel along each axis, from 1 to [`MAX_SAMPLES`],
    /// so 2 shades each pixel four times and averages the result. One sample
    /// leaves the highlights speckled wherever the surface is steep, since
    /// they are finer there than a pixel; every extra sample costs its share
    /// of the render.
    pub samples: usize,
}

impl Default for Globe {
    fn default() -> Self {
        Self {
            surface: Relief {
                // A sphere is read at a glance, so it carries much taller
                // bumps than the flat relief. Those bumps leave steep walls,
                // which want a wider height curve to fall smoothly, deeper
                // occlusion to reach the dark, and a tighter highlight so a
                // whole wall does not light up grey at once.
                height: 0.9,
                gamma: 1.6,
                occlusion: 0.7,
                ambient: 0.3,
                diffuse: 1.2,
                gloss: 300.0,
                ..Default::default()
            },
            rgb_from_fourth_axis: true,
            fill: 0.92,
            background: [7, 8, 12],
            samples: 2,
        }
    }
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
    pub(crate) fn apply(&self, value: f64) -> f64 {
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
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) channels: usize,
    pub(crate) pixels: Vec<u8>,
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
