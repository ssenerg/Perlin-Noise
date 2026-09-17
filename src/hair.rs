//! Hair grown by turning a two dimensional field loose on particles.
//!
//! Every other mode reads the lattice as a picture. This one reads it as a
//! force: a particle dropped into the field is pulled by the gradients at the
//! corners of whatever cell it is in, blended by the same fade curve
//! [`Instance::noise`](crate::Instance::noise) uses, and the path it takes is
//! drawn as one strand. A few thousand particles later the picture is a coat
//! of hair lying along the field.
//!
//! The particles obey `dv/dt = force - drag * v` and `dp/dt = v`, so they
//! carry momentum: a strand does not turn as sharply as the field under it
//! does, which is what keeps the hair smooth rather than frizzy. Drag is
//! integrated exactly instead of by an Euler step, so the step size only
//! decides how finely a path is traced, never whether it blows up.
//!
//! What makes the result read as hair rather than as a smooth picture of a
//! flow is that the strands are drawn as a coat rather than added together.
//! Every one is given a depth, and they are laid down back to front, each
//! opaque enough to hide what it crosses. The ones underneath are darker for
//! being buried, and no two are quite alike: they differ in how bright, how
//! thick and how long they are, which is what keeps neighbours from merging
//! into one wash.

use std::io::{Error, ErrorKind, Result};

use crate::image::{in_parallel, to_linear, to_srgb};
use crate::rng::SplitMix64;
use crate::{Image, Palette, Relief, Renderer};

/// Strands below which the simulation stays on one thread. Growing one is
/// thousands of noise samples, so it pays off far sooner than shading a pixel
/// does.
const PARALLEL_STRANDS: usize = 64;

/// Fewest rows worth handing a thread of its own when the strands are drawn.
const LEAST_ROWS: usize = 64;

/// Steps the lighting is worked out at before any strand is drawn, over the
/// range of angles one can meet the light at.
const ANGLES: usize = 1024;

impl Renderer {
    /// A coat of hair drawn from the paths of particles set loose in a two
    /// dimensional field, on a `width` by `height` image.
    ///
    /// `hair.count` particles start at seeded random points of the lattice
    /// box and are pushed around by it for `hair.steps` steps. Each path is
    /// stroked into the picture at its own depth, front strands over back
    /// ones. The box is stretched to fill the image, so `dims` should carry
    /// the image's aspect ratio if the strands are not to come out stretched
    /// with it; a particle that walks out of the box ends its strand there.
    ///
    /// Like [`Renderer::globe`] and unlike the lattice modes, this follows
    /// paths rather than sample points, so it takes a size in pixels, has no
    /// GPU path, and `divisors` play no part: `dims` alone set how far a
    /// strand travels before the field turns it.
    pub fn hair(&self, width: usize, height: usize, hair: &Hair) -> Result<Image> {
        self.expect_dims(2)?;
        if width == 0 || height == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "An image needs a positive width and height",
            ));
        }
        hair.check()?;

        let dims = self.noise.dims();
        // The lattice box stretched over the image. The first axis runs down
        // the image and the second across it, as everywhere else.
        let scale = [
            width as f64 / dims[1] as f64,
            height as f64 / dims[0] as f64,
        ];

        let mut strands = vec![Strand::default(); hair.count];
        in_parallel(&mut strands, PARALLEL_STRANDS, |index, strand| {
            *strand = self.grow(hair, index, scale);
        });

        // Back to front, so a strand covers whatever lies deeper than it and
        // the coat closes over itself the way a real one does.
        let mut order: Vec<usize> = (0..strands.len()).collect();
        order.sort_unstable_by(|one, other| strands[*one].depth.total_cmp(&strands[*other].depth));

        // Strands cross, so threads cannot each take a share of them without
        // painting over one another out of order. They take a band of rows
        // instead and each lays down the whole coat, skipping the strands
        // that never reach its own band.
        // The relief of this same field, when the coat is to be coloured by
        // it: a colour map the strands are painted with, rather than a ramp
        // the lighting walks on its own.
        let map = if hair.color_from_field {
            Some(self.relief_map(width, height, &hair.surface)?)
        } else {
            None
        };
        let look = Look::of(hair, map, width, height);
        let mut bands = Canvas::bands(width, height, look.background());
        std::thread::scope(|scope| {
            for canvas in &mut bands {
                let (strands, order, look) = (&strands, &order, &look);
                scope.spawn(move || canvas.comb(strands, order, hair, look));
            }
        });

        Ok(Image {
            width,
            height,
            channels: 3,
            pixels: bands
                .into_iter()
                .flat_map(|canvas| canvas.develop())
                .collect(),
        })
    }

    /// One particle's path through the field, and what the strand drawn along
    /// it looks like.
    fn grow(&self, hair: &Hair, index: usize, scale: [f64; 2]) -> Strand {
        let dims = self.noise.dims();
        let box_size = [dims[0] as f64, dims[1] as f64];
        // Mixed rather than offset so that neighbouring seeds do not hand
        // neighbouring particles the same starting points.
        let mut random = SplitMix64::new(
            hair.seed
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(index as u64),
        );
        let mut at = [
            random.next_f64() * box_size[0],
            random.next_f64() * box_size[1],
        ];

        // No two strands alike. One dim strand among bright ones is what lets
        // the eye pick a single hair out of a coat of them, so this is most of
        // the difference between hair and a smooth wash of the same shape.
        let depth = random.next_f64();
        // Dulled for being a poorer hair than its neighbours, and again for
        // how much of the coat is piled on top of it.
        let dim = (1.0 - hair.jitter * random.next_f64()) * (1.0 - hair.depth * (1.0 - depth));
        let radius = (hair.thickness / 2.0) * (1.0 - 0.5 * hair.jitter * random.next_f64());
        let steps = hair.steps as f64 * (1.0 - hair.jitter * random.next_f64());
        // A push of its own, held for the whole of its length, which is what
        // sets a strand off across its neighbours instead of beside them.
        let stray = random.next_unit_vector(2);
        let stray = [
            hair.frizz * hair.force * stray[0],
            hair.frizz * hair.force * stray[1],
        ];

        let mut strand = Strand {
            path: Vec::with_capacity(steps as usize + 1),
            depth: depth as f32,
            dim: dim as f32,
            radius,
            top: f64::INFINITY,
            bottom: f64::NEG_INFINITY,
        };

        let mut speed = [0.0f64; 2];
        let (turn_sin, turn_cos) = hair.swirl.to_radians().sin_cos();
        // How much of its speed a particle keeps over one step. Constant, so
        // the exact solution of the drag costs one exponential per strand.
        let decay = (-hair.drag * hair.step).exp();

        strand.reach([at[1] * scale[0], at[0] * scale[1]]);
        let mut pull = [0.0f64; 2];
        for _ in 0..steps as usize {
            self.noise.pull(&at, &mut pull);
            // Turning the force is what decides whether the particles run
            // into the field's sinks or circle them.
            let force = [
                hair.force * (pull[0] * turn_cos - pull[1] * turn_sin) + stray[0],
                hair.force * (pull[0] * turn_sin + pull[1] * turn_cos) + stray[1],
            ];

            let was = speed;
            speed = if hair.drag > 0.0 {
                // Where the force alone would hold the particle, approached
                // by whatever fraction of the old speed the drag ate.
                let terminal = [force[0] / hair.drag, force[1] / hair.drag];
                [
                    was[0] * decay + terminal[0] * (1.0 - decay),
                    was[1] * decay + terminal[1] * (1.0 - decay),
                ]
            } else {
                [was[0] + force[0] * hair.step, was[1] + force[1] * hair.step]
            };
            // Averaging the speed either side of the step keeps the path
            // second order accurate, so it curves rather than cutting corners.
            for axis in 0..2 {
                at[axis] += 0.5 * (was[axis] + speed[axis]) * hair.step;
            }

            if !at[0].is_finite() || !at[1].is_finite() {
                break;
            }
            // Outside the lattice there is no field to follow, so the strand
            // ends where it leaves the picture.
            if at[0] < 0.0 || at[0] > box_size[0] || at[1] < 0.0 || at[1] > box_size[1] {
                break;
            }
            strand.reach([at[1] * scale[0], at[0] * scale[1]]);
        }

        strand
    }

    /// The lit relief of this field, sampled as densely as the hair image, so
    /// a strand can read a colour off the same picture [`Renderer::relief`]
    /// would have drawn.
    fn relief_map(&self, width: usize, height: usize, surface: &Relief) -> Result<ColorMap> {
        let dims = self.noise.dims();
        let divisors = [
            (width.saturating_sub(1) / dims[0]).max(1),
            (height.saturating_sub(1) / dims[1]).max(1),
        ];
        let image = self.relief(&divisors, surface)?;
        Ok(ColorMap::from_image(&image))
    }
}

/// Where the hair comes from and what it looks like, for [`Renderer::hair`].
///
/// [`Hair::default`] is a dense grey coat lit from the top left. The particles
/// are worth thinking of together: `steps` times `step` is how long each one
/// is followed, and `force` over `drag` is roughly how fast it ends up going,
/// so their product is about how far a strand travels in lattice cells.
#[derive(Clone, Copy, Debug)]
pub struct Hair {
    /// How many particles are set loose, one strand each.
    pub count: usize,
    /// How many steps each particle is followed for, before `jitter` takes
    /// some of them off again.
    pub steps: usize,
    /// How much time one step covers. Smaller traces the same path more
    /// finely rather than changing it, so lower this only if the strands look
    /// like chains of straight pieces.
    pub step: f64,
    /// How hard the lattice pulls. The blend of the surrounding gradients is
    /// never longer than 1, so this is the strongest force a particle can
    /// meet.
    pub force: f64,
    /// How quickly a particle loses the speed it has, which is what stops it
    /// accelerating for ever. Low values leave enough momentum for a particle
    /// to overshoot and circle; high values pin it to the field's own
    /// direction, which gives shorter, more orderly hair.
    pub drag: f64,
    /// Degrees the force is turned through before it is applied. At 0 the
    /// particles run straight into whatever the lattice points at and pile up
    /// there; near 90 they orbit it instead, which draws long curling hair
    /// with no bald patches between the clumps.
    pub swirl: f64,
    /// How thick a strand is drawn, in pixels, before `jitter` thins some of
    /// them.
    pub thickness: f64,
    /// How much of what lies under it a strand hides, 0.0 to 1.0. Below about
    /// 0.8 the coat starts to look like one thing seen through gauze rather
    /// than like hairs lying over each other.
    pub opacity: f64,
    /// How hard each strand is pushed in a direction of its own, as a
    /// fraction of `force`. Particles that shared a starting point would
    /// otherwise follow the very same path, so the strands lie perfectly
    /// parallel and the coat comes out combed flat. A little of this sets
    /// them across each other the way real hair lies; past about 0.3 the
    /// field stops being able to hold them and the coat turns to fluff.
    pub frizz: f64,
    /// How unlike each other the strands are, 0.0 to 1.0: how much of its
    /// brightness, thickness and length any one of them may be short of the
    /// figures above. At 0 they are identical and merge into a smooth wash,
    /// which is the one setting that stops this looking like hair at all.
    pub jitter: f64,
    /// How much darker a strand right at the back of the coat is than one at
    /// the front, 0.0 to 1.0. This is what gives the coat depth: the dark
    /// between the lit strands is the hair buried under them.
    pub depth: f64,
    /// Fraction of a strand over which it narrows to a point at the root and
    /// at the tip, from 0.0 to 0.5.
    pub taper: f64,
    /// Where the light comes from, in image space: x to the right, y down the
    /// image. Length does not matter.
    pub light: [f64; 2],
    /// Brightness of a strand lying along the light, 0.0 to 1.0.
    pub ambient: f64,
    /// Brightness of a strand lying across it, 0.0 to 1.0.
    pub diffuse: f64,
    /// Strength of the sheen, the band of light that runs over hair where it
    /// turns square to the light, 0.0 to 1.0.
    pub specular: f64,
    /// Tightness of that band. Higher is a narrower, brighter stripe.
    pub gloss: f64,
    /// Paint the coat with [`Renderer::relief`] of this same field, read as a
    /// colour map. Each strand keeps the lighting of a fibre, but the colour
    /// it is lighting is the glossy height map rather than a grey ramp, so
    /// the picture is the relief with hair on it. [`Hair::surface`] is the
    /// relief that map is drawn with; left at its default it is the crimson
    /// liquid look.
    ///
    /// While this is off the colour comes from the lighting alone, so
    /// `palette` is a ramp from dark hair to lit hair and the field shows only
    /// in which way the strands run.
    pub color_from_field: bool,
    /// How the colour map is shaped, paletted and lit when `color_from_field`
    /// is on. Ignored otherwise.
    pub surface: Relief,
    /// Colour ramp from the background at 0.0 to a strand in full sheen at
    /// 1.0. Unused while `color_from_field` is on, because the colour then
    /// comes from [`Hair::surface`].
    pub palette: Palette,
    /// Seed for where the particles start and for what each strand looks
    /// like, which is separate from the seed the field itself was built from.
    pub seed: u64,
}

impl Default for Hair {
    fn default() -> Self {
        Self {
            count: 45_000,
            steps: 170,
            step: 0.05,
            force: 1.0,
            drag: 3.0,
            // Enough of a turn that the particles sweep past the places the
            // field points at rather than collecting in them, but not so much
            // that they only ever go round in circles.
            swirl: 62.0,
            thickness: 1.3,
            opacity: 0.95,
            frizz: 0.16,
            jitter: 0.6,
            depth: 0.6,
            taper: 0.22,
            light: [-0.55, -0.55],
            ambient: 0.13,
            // A modest spread of light with a tight band of sheen over it,
            // which is how hair catches the light: were the diffuse term
            // carrying it instead, every strand facing the same way would
            // flare at once and the coat would read as a lit surface.
            diffuse: 0.5,
            specular: 0.62,
            gloss: 14.0,
            color_from_field: false,
            // Flatter than the standalone relief: the coat already has volume
            // from the strands, and the full height map's lighting buried the
            // hair under huge highlights.
            surface: Relief {
                height: 0.12,
                ..Relief::default()
            },
            palette: Palette::Gray,
            seed: 1,
        }
    }
}

impl Hair {
    fn check(&self) -> Result<()> {
        let bad = |message: String| Err(Error::new(ErrorKind::InvalidInput, message));
        if !(self.step > 0.0 && self.step.is_finite()) {
            return bad(format!("The step has to be above 0, got {}", self.step));
        }
        if !(self.drag >= 0.0 && self.drag.is_finite()) {
            return bad(format!("Drag cannot be negative, got {}", self.drag));
        }
        if !self.force.is_finite() {
            return bad(format!("The force has to be a number, got {}", self.force));
        }
        if !(self.thickness > 0.0 && self.thickness.is_finite()) {
            return bad(format!(
                "Thickness has to be above 0, got {}",
                self.thickness
            ));
        }
        if !(0.0..=1.0).contains(&self.opacity) {
            return bad(format!("Opacity has to be 0 to 1, got {}", self.opacity));
        }
        if !(self.frizz >= 0.0 && self.frizz.is_finite()) {
            return bad(format!("Frizz cannot be negative, got {}", self.frizz));
        }
        if !(0.0..=1.0).contains(&self.jitter) {
            return bad(format!("Jitter has to be 0 to 1, got {}", self.jitter));
        }
        if !(0.0..=1.0).contains(&self.depth) {
            return bad(format!("Depth has to be 0 to 1, got {}", self.depth));
        }
        if !(0.0..=0.5).contains(&self.taper) {
            return bad(format!("Taper has to be 0 to 0.5, got {}", self.taper));
        }
        if !(0.0..=1.0).contains(&self.ambient) {
            return bad(format!("Ambient has to be 0 to 1, got {}", self.ambient));
        }
        if !(0.0..=1.0).contains(&self.diffuse) {
            return bad(format!("Diffuse has to be 0 to 1, got {}", self.diffuse));
        }
        if !(0.0..=1.0).contains(&self.specular) {
            return bad(format!("Specular has to be 0 to 1, got {}", self.specular));
        }
        if !(self.gloss > 0.0 && self.gloss.is_finite()) {
            return bad(format!("Gloss has to be above 0, got {}", self.gloss));
        }
        Ok(())
    }
}

/// One particle's path and the strand drawn along it.
#[derive(Clone, Default)]
struct Strand {
    /// Where the particle went, in pixels: x across the image, y down it.
    path: Vec<[f32; 2]>,
    /// Where it lies in the coat, 0.0 at the very back and 1.0 at the front.
    depth: f32,
    /// What this strand keeps of the light falling on it, once being a duller
    /// hair than its neighbours and being buried under them are both counted.
    dim: f32,
    /// Half its thickness, in pixels.
    radius: f64,
    /// The rows it reaches, so a band can pass over it in one comparison.
    top: f64,
    bottom: f64,
}

impl Strand {
    fn reach(&mut self, at: [f64; 2]) {
        self.top = self.top.min(at[1]);
        self.bottom = self.bottom.max(at[1]);
        self.path.push([at[0] as f32, at[1] as f32]);
    }
}

/// The lit relief, held in linear light so a strand can sample it without
/// decoding a colour byte every time.
struct ColorMap {
    width: usize,
    height: usize,
    pixels: Vec<[f32; 3]>,
}

impl ColorMap {
    fn from_image(image: &Image) -> Self {
        Self {
            width: image.width(),
            height: image.height(),
            pixels: image
                .pixels()
                .chunks(3)
                .map(|pixel| {
                    [
                        to_linear(pixel[0]) as f32,
                        to_linear(pixel[1]) as f32,
                        to_linear(pixel[2]) as f32,
                    ]
                })
                .collect(),
        }
    }

    /// Colour at a pixel of the hair image, bilinearly from this map, which
    /// may be a few pixels off the image size because relief is sampled on
    /// the lattice.
    fn sample(&self, x: f64, y: f64, dst_w: usize, dst_h: usize) -> [f32; 3] {
        if self.width == 0 || self.height == 0 || dst_w == 0 || dst_h == 0 {
            return [0.0; 3];
        }
        let u = (x + 0.5) * self.width as f64 / dst_w as f64 - 0.5;
        let v = (y + 0.5) * self.height as f64 / dst_h as f64 - 0.5;
        let x0 = u.floor() as i32;
        let y0 = v.floor() as i32;
        let tx = (u - f64::from(x0)) as f32;
        let ty = (v - f64::from(y0)) as f32;
        let pix = |col: i32, row: i32| {
            let col = col.clamp(0, self.width as i32 - 1) as usize;
            let row = row.clamp(0, self.height as i32 - 1) as usize;
            self.pixels[row * self.width + col]
        };
        let mix = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };
        let top = mix(pix(x0, y0), pix(x0 + 1, y0), tx);
        let bottom = mix(pix(x0, y0 + 1), pix(x0 + 1, y0 + 1), tx);
        mix(top, bottom, ty)
    }
}

/// Everything about a strand's colour that can be worked out before there are
/// any strands: what a brightness looks like, and how brightly a strand lying
/// at a given angle to the light is lit.
///
/// Both are tables because the alternative is a `powf` and three of them per
/// segment of every strand, repeated in every band.
struct Look {
    /// The palette in linear light, one entry per byte of the ramp, used when
    /// the coat is grey and the lighting picks the colour.
    ramp: [[f32; 3]; 256],
    /// The broad wash of light against how squarely a strand lies across it.
    wash: [f32; ANGLES],
    /// The narrow band of sheen riding on that wash, against the same.
    sheen: [f32; ANGLES],
    /// The relief of the field, when the coat is coloured by it.
    map: Option<ColorMap>,
    /// Size of the hair image, which is what the map is sampled in.
    width: usize,
    height: usize,
}

impl Look {
    fn of(hair: &Hair, map: Option<ColorMap>, width: usize, height: usize) -> Self {
        let mut ramp = [[0.0; 3]; 256];
        for (step, color) in ramp.iter_mut().enumerate() {
            let position = step as f64 / 255.0;
            for (channel, byte) in color.iter_mut().zip(hair.palette.color(position)) {
                *channel = to_linear(byte) as f32;
            }
        }

        let mut wash = [0.0; ANGLES];
        let mut sheen = [0.0; ANGLES];
        for (step, (wash, sheen)) in wash.iter_mut().zip(&mut sheen).enumerate() {
            let across = step as f64 / (ANGLES - 1) as f64;
            *wash = (hair.ambient + hair.diffuse * across) as f32;
            *sheen = (hair.specular * across.powf(hair.gloss)) as f32;
        }

        Self {
            ramp,
            wash,
            sheen,
            map,
            width,
            height,
        }
    }

    fn background(&self) -> [f32; 3] {
        if self.map.is_some() {
            // The grey coat sits on black, and the coloured one should too:
            // using the bottom of the relief as the background paints a flat
            // slab of colour behind the hair.
            [0.0; 3]
        } else {
            self.ramp[0]
        }
    }

    /// The colour of a strand lying `across` the light at `at` in the image,
    /// after `dim` has taken its share for the strand being a dull one or a
    /// buried one.
    fn color(&self, across: f64, dim: f32, at: [f64; 2]) -> [f32; 3] {
        let angle = (across.clamp(0.0, 1.0) * (ANGLES - 1) as f64) as usize;
        if let Some(map) = &self.map {
            // The relief is already a lit colour, highlights and all. A
            // second white sheen on top of those highlights turns the coat
            // silver, so the fibre lighting only says how much of that colour
            // comes back: a strand lying along the light goes dark, one
            // crossing it shows the map.
            let tint = map.sample(at[0], at[1], self.width, self.height);
            let fibre = (self.wash[0] + (1.0 - self.wash[0]) * across.clamp(0.0, 1.0) as f32) * dim;
            let mut color = [0.0; 3];
            for (channel, tint) in color.iter_mut().zip(tint) {
                *channel = tint * fibre;
            }
            color
        } else {
            let (wash, sheen) = (self.wash[angle], self.sheen[angle]);
            let lit = (wash + sheen) * dim;
            self.ramp[(lit.clamp(0.0, 1.0) * 255.0) as usize]
        }
    }
}

/// A band of rows of the picture, in linear light, with the strands that
/// reach it painted on.
struct Canvas {
    width: usize,
    /// Row of the whole image this band starts at.
    top: usize,
    /// How many rows it covers.
    rows: usize,
    pixels: Vec<[f32; 3]>,
}

impl Canvas {
    /// The picture cut into one band per thread, top to bottom, with nothing
    /// on it but the background.
    fn bands(width: usize, height: usize, background: [f32; 3]) -> Vec<Self> {
        let threads = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            // Every band passes over every strand to find the ones that reach
            // it, so bands thinner than this cost more in the passing over
            // than they save in the painting.
            .clamp(1, height.div_ceil(LEAST_ROWS));
        let band = height.div_ceil(threads).max(1);

        (0..height.div_ceil(band))
            .map(|index| {
                let top = index * band;
                let rows = band.min(height - top);
                Self {
                    width,
                    top,
                    rows,
                    pixels: vec![background; width * rows],
                }
            })
            .collect()
    }

    /// Lays down every strand that reaches this band, in the order given.
    fn comb(&mut self, strands: &[Strand], order: &[usize], hair: &Hair, look: &Look) {
        let light = direction(hair.light);
        let reach = hair.thickness / 2.0 + 1.0;
        let (above, below) = (
            self.top as f64 - reach,
            (self.top + self.rows) as f64 + reach,
        );

        for index in order {
            let strand = &strands[*index];
            if strand.bottom < above || strand.top > below {
                continue;
            }

            let segments = strand.path.len().saturating_sub(1);
            for (step, pair) in strand.path.windows(2).enumerate() {
                let from = [f64::from(pair[0][0]), f64::from(pair[0][1])];
                let to = [f64::from(pair[1][0]), f64::from(pair[1][1])];

                // A strand narrows to a point at the root and at the tip, so
                // it grows out of the picture and dies away again instead of
                // being cut off square at both ends. Once it is finer than a
                // pixel it goes on thinning by turning see-through, since
                // there is no width left to take away.
                let along = (step as f64 + 0.5) / segments as f64;
                let taper = if hair.taper > 0.0 {
                    (along / hair.taper)
                        .min((1.0 - along) / hair.taper)
                        .min(1.0)
                } else {
                    1.0
                };
                let radius = strand.radius * taper;
                let (radius, thinned) = if radius < 0.5 {
                    (0.5, radius / 0.5)
                } else {
                    (radius, 1.0)
                };

                // How squarely the strand lies across the light. A fibre is
                // lit by that rather than by which way it faces, so one
                // running along the light goes dark and one crossing it lights
                // up; a curving strand passes through both, which is the band
                // of sheen that runs over hair.
                let tangent = direction([to[0] - from[0], to[1] - from[1]]);
                let across = 1.0 - (tangent[0] * light[0] + tangent[1] * light[1]).powi(2);
                let at = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5];

                self.stroke(
                    from,
                    to,
                    radius,
                    hair.opacity * thinned,
                    look.color(across.max(0.0).sqrt(), strand.dim, at),
                );
            }
        }
    }

    /// Paints one segment of a strand over whatever is already there.
    fn stroke(&mut self, from: [f64; 2], to: [f64; 2], radius: f64, opacity: f64, color: [f32; 3]) {
        // Half a pixel past the edge, which is where the last pixel the
        // strand touches at all has its centre.
        let reach = radius + 0.5;
        let (left, right) = (from[0].min(to[0]) - reach, from[0].max(to[0]) + reach);
        let (top, bottom) = (from[1].min(to[1]) - reach, from[1].max(to[1]) + reach);
        if !(right >= 0.0 && left < self.width as f64 && opacity > 0.0) {
            return;
        }
        if !(bottom >= self.top as f64 && top < (self.top + self.rows) as f64) {
            return;
        }

        // A particle that stood still over the step left no strand to paint.
        let delta = [to[0] - from[0], to[1] - from[1]];
        let length = delta[0] * delta[0] + delta[1] * delta[1];
        if length <= 0.0 || length.is_nan() {
            return;
        }

        let first = |edge: f64, low: usize| edge.floor().max(low as f64) as usize;
        let last = |edge: f64, high: usize| edge.ceil().min(high as f64) as usize;

        for row in first(top, self.top)..=last(bottom, self.top + self.rows - 1) {
            for col in first(left, 0)..=last(right, self.width - 1) {
                let at = [col as f64 + 0.5, row as f64 + 0.5];
                // Distance from the pixel to the segment, taken to the
                // nearest point along it so the round ends meet cleanly.
                let along = (((at[0] - from[0]) * delta[0] + (at[1] - from[1]) * delta[1])
                    / length)
                    .clamp(0.0, 1.0);
                let aside = [
                    at[0] - (from[0] + along * delta[0]),
                    at[1] - (from[1] + along * delta[1]),
                ];
                let distance = (aside[0] * aside[0] + aside[1] * aside[1]).sqrt();

                // Coverage falls off over the pixel straddling the edge,
                // which is what keeps a one pixel hair smooth rather than
                // stepped.
                let cover = (reach - distance).clamp(0.0, 1.0);
                if cover <= 0.0 {
                    continue;
                }

                let alpha = (cover * opacity) as f32;
                let under = &mut self.pixels[(row - self.top) * self.width + col];
                for (behind, front) in under.iter_mut().zip(color) {
                    *behind += (front - *behind) * alpha;
                }
            }
        }
    }

    /// Turns the band's light into pixels.
    fn develop(self) -> Vec<u8> {
        self.pixels
            .into_iter()
            .flatten()
            .map(|channel| to_srgb(f64::from(channel)))
            .collect()
    }
}

/// A unit vector, falling back on one pointing right when there is no
/// direction to speak of.
fn direction(vector: [f64; 2]) -> [f64; 2] {
    let norm = (vector[0] * vector[0] + vector[1] * vector[1]).sqrt();
    if norm > 0.0 && norm.is_finite() {
        [vector[0] / norm, vector[1] / norm]
    } else {
        [1.0, 0.0]
    }
}
