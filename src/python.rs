//! Private PyO3 module. The documented Python API lives in
//! `python/perlin_noise/__init__.py` and re-exports these types.

use std::io::{Error, ErrorKind};
use std::path::PathBuf;

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::{Globe, Hair, Image, Instance, MAX_DIMS, Palette, Relief, Renderer, Shape, Table};

fn io_err(err: Error) -> PyErr {
    if err.kind() == ErrorKind::InvalidInput {
        PyValueError::new_err(err.to_string())
    } else {
        PyIOError::new_err(err.to_string())
    }
}

fn pair3(values: Vec<f64>, what: &str) -> PyResult<[f64; 3]> {
    let [x, y, z] = values.as_slice() else {
        return Err(PyValueError::new_err(format!(
            "{what} wants three numbers, got {values:?}"
        )));
    };
    Ok([*x, *y, *z])
}

fn pair2(values: Vec<f64>, what: &str) -> PyResult<[f64; 2]> {
    let [x, y] = values.as_slice() else {
        return Err(PyValueError::new_err(format!(
            "{what} wants two numbers, got {values:?}"
        )));
    };
    Ok([*x, *y])
}

fn rgb(values: Vec<u8>, what: &str) -> PyResult<[u8; 3]> {
    let [r, g, b] = values.as_slice() else {
        return Err(PyValueError::new_err(format!(
            "{what} wants three bytes, got {values:?}"
        )));
    };
    Ok([*r, *g, *b])
}

/// Maps a noise value to a colour.
///
/// ``GRAY`` is black through white. ``BLUE_RED`` is blue through white to red.
/// ``HEAT`` is black through red and orange to white. ``CRIMSON`` and
/// ``AZURE`` are the ramps built for relief: they hold no black, so the dark
/// comes only from the shading. ``TERRAIN`` is water, sand, grass, rock, snow.
#[pyclass(eq, eq_int, frozen, name = "Palette", module = "perlin_noise")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PyPalette {
    #[pyo3(name = "GRAY")]
    Gray,
    #[pyo3(name = "BLUE_RED")]
    BlueRed,
    #[pyo3(name = "HEAT")]
    Heat,
    #[pyo3(name = "CRIMSON")]
    Crimson,
    #[pyo3(name = "AZURE")]
    Azure,
    #[pyo3(name = "TERRAIN")]
    Terrain,
}

impl From<PyPalette> for Palette {
    fn from(value: PyPalette) -> Self {
        match value {
            PyPalette::Gray => Palette::Gray,
            PyPalette::BlueRed => Palette::BlueRed,
            PyPalette::Heat => Palette::Heat,
            PyPalette::Crimson => Palette::Crimson,
            PyPalette::Azure => Palette::Azure,
            PyPalette::Terrain => Palette::Terrain,
        }
    }
}

impl From<Palette> for PyPalette {
    fn from(value: Palette) -> Self {
        match value {
            Palette::Gray => PyPalette::Gray,
            Palette::BlueRed => PyPalette::BlueRed,
            Palette::Heat => PyPalette::Heat,
            Palette::Crimson => PyPalette::Crimson,
            Palette::Azure => PyPalette::Azure,
            Palette::Terrain => PyPalette::Terrain,
        }
    }
}

#[pymethods]
impl PyPalette {
    /// Colour at ``position``, which is clamped to 0..1.
    fn color(&self, position: f64) -> (u8, u8, u8) {
        let [r, g, b] = Palette::from(*self).color(position);
        (r, g, b)
    }

    fn __repr__(&self) -> String {
        format!("Palette.{self:?}")
    }
}

/// How the noise is shaped into a height map before it is lit.
///
/// ``SMOOTH`` is rolling hills. ``BILLOW`` turns the zero-crossings into
/// creases between rounded blobs. ``RIDGED`` is billow upside down: sharp
/// ridges with wide basins between them.
#[pyclass(eq, eq_int, frozen, name = "Shape", module = "perlin_noise")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PyShape {
    #[pyo3(name = "SMOOTH")]
    Smooth,
    #[pyo3(name = "BILLOW")]
    Billow,
    #[pyo3(name = "RIDGED")]
    Ridged,
}

impl From<PyShape> for Shape {
    fn from(value: PyShape) -> Self {
        match value {
            PyShape::Smooth => Shape::Smooth,
            PyShape::Billow => Shape::Billow,
            PyShape::Ridged => Shape::Ridged,
        }
    }
}

impl From<Shape> for PyShape {
    fn from(value: Shape) -> Self {
        match value {
            Shape::Smooth => PyShape::Smooth,
            Shape::Billow => PyShape::Billow,
            Shape::Ridged => PyShape::Ridged,
        }
    }
}

#[pymethods]
impl PyShape {
    fn __repr__(&self) -> String {
        format!("Shape.{self:?}")
    }
}

/// Lighting and colour for a height map.
///
/// The defaults are the glossy liquid look: billowing blobs, crimson, lit
/// from the top left. Fields are tuned by eye.
#[pyclass(name = "Relief", module = "perlin_noise")]
#[derive(Clone)]
pub struct PyRelief {
    inner: Relief,
}

impl PyRelief {
    fn from_inner(inner: Relief) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyRelief {
    #[new]
    #[pyo3(signature = (
        shape=None,
        palette=None,
        light=None,
        height=None,
        gamma=None,
        occlusion=None,
        ambient=None,
        diffuse=None,
        specular=None,
        gloss=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        shape: Option<PyShape>,
        palette: Option<PyPalette>,
        light: Option<Vec<f64>>,
        height: Option<f64>,
        gamma: Option<f64>,
        occlusion: Option<f64>,
        ambient: Option<f64>,
        diffuse: Option<f64>,
        specular: Option<f64>,
        gloss: Option<f64>,
    ) -> PyResult<Self> {
        let mut inner = Relief::default();
        if let Some(shape) = shape {
            inner.shape = shape.into();
        }
        if let Some(palette) = palette {
            inner.palette = palette.into();
        }
        if let Some(light) = light {
            inner.light = pair3(light, "light")?;
        }
        if let Some(height) = height {
            inner.height = height;
        }
        if let Some(gamma) = gamma {
            inner.gamma = gamma;
        }
        if let Some(occlusion) = occlusion {
            inner.occlusion = occlusion;
        }
        if let Some(ambient) = ambient {
            inner.ambient = ambient;
        }
        if let Some(diffuse) = diffuse {
            inner.diffuse = diffuse;
        }
        if let Some(specular) = specular {
            inner.specular = specular;
        }
        if let Some(gloss) = gloss {
            inner.gloss = gloss;
        }
        Ok(Self { inner })
    }

    #[getter]
    fn shape(&self) -> PyShape {
        self.inner.shape.into()
    }

    #[setter]
    fn set_shape(&mut self, value: PyShape) {
        self.inner.shape = value.into();
    }

    #[getter]
    fn palette(&self) -> PyPalette {
        self.inner.palette.into()
    }

    #[setter]
    fn set_palette(&mut self, value: PyPalette) {
        self.inner.palette = value.into();
    }

    #[getter]
    fn light(&self) -> (f64, f64, f64) {
        let [x, y, z] = self.inner.light;
        (x, y, z)
    }

    #[setter]
    fn set_light(&mut self, value: Vec<f64>) -> PyResult<()> {
        self.inner.light = pair3(value, "light")?;
        Ok(())
    }

    #[getter]
    fn height(&self) -> f64 {
        self.inner.height
    }

    #[setter]
    fn set_height(&mut self, value: f64) {
        self.inner.height = value;
    }

    #[getter]
    fn gamma(&self) -> f64 {
        self.inner.gamma
    }

    #[setter]
    fn set_gamma(&mut self, value: f64) {
        self.inner.gamma = value;
    }

    #[getter]
    fn occlusion(&self) -> f64 {
        self.inner.occlusion
    }

    #[setter]
    fn set_occlusion(&mut self, value: f64) {
        self.inner.occlusion = value;
    }

    #[getter]
    fn ambient(&self) -> f64 {
        self.inner.ambient
    }

    #[setter]
    fn set_ambient(&mut self, value: f64) {
        self.inner.ambient = value;
    }

    #[getter]
    fn diffuse(&self) -> f64 {
        self.inner.diffuse
    }

    #[setter]
    fn set_diffuse(&mut self, value: f64) {
        self.inner.diffuse = value;
    }

    #[getter]
    fn specular(&self) -> f64 {
        self.inner.specular
    }

    #[setter]
    fn set_specular(&mut self, value: f64) {
        self.inner.specular = value;
    }

    #[getter]
    fn gloss(&self) -> f64 {
        self.inner.gloss
    }

    #[setter]
    fn set_gloss(&mut self, value: f64) {
        self.inner.gloss = value;
    }

    fn __repr__(&self) -> String {
        format!(
            "Relief(shape={:?}, palette={:?}, height={})",
            self.inner.shape, self.inner.palette, self.inner.height
        )
    }
}

/// Lighting and colour for a sphere carved out of a four dimensional field.
#[pyclass(name = "Globe", module = "perlin_noise")]
#[derive(Clone)]
pub struct PyGlobe {
    inner: Globe,
}

#[pymethods]
impl PyGlobe {
    #[new]
    #[pyo3(signature = (
        surface=None,
        rgb_from_fourth_axis=None,
        fill=None,
        background=None,
        samples=None,
    ))]
    fn new(
        surface: Option<PyRelief>,
        rgb_from_fourth_axis: Option<bool>,
        fill: Option<f64>,
        background: Option<Vec<u8>>,
        samples: Option<usize>,
    ) -> PyResult<Self> {
        let mut inner = Globe::default();
        if let Some(surface) = surface {
            inner.surface = surface.inner;
        }
        if let Some(rgb_from_fourth_axis) = rgb_from_fourth_axis {
            inner.rgb_from_fourth_axis = rgb_from_fourth_axis;
        }
        if let Some(fill) = fill {
            inner.fill = fill;
        }
        if let Some(background) = background {
            inner.background = rgb(background, "background")?;
        }
        if let Some(samples) = samples {
            inner.samples = samples;
        }
        Ok(Self { inner })
    }

    #[getter]
    fn surface(&self) -> PyRelief {
        PyRelief::from_inner(self.inner.surface)
    }

    #[setter]
    fn set_surface(&mut self, value: PyRelief) {
        self.inner.surface = value.inner;
    }

    #[getter]
    fn rgb_from_fourth_axis(&self) -> bool {
        self.inner.rgb_from_fourth_axis
    }

    #[setter]
    fn set_rgb_from_fourth_axis(&mut self, value: bool) {
        self.inner.rgb_from_fourth_axis = value;
    }

    #[getter]
    fn fill(&self) -> f64 {
        self.inner.fill
    }

    #[setter]
    fn set_fill(&mut self, value: f64) {
        self.inner.fill = value;
    }

    #[getter]
    fn background(&self) -> (u8, u8, u8) {
        let [r, g, b] = self.inner.background;
        (r, g, b)
    }

    #[setter]
    fn set_background(&mut self, value: Vec<u8>) -> PyResult<()> {
        self.inner.background = rgb(value, "background")?;
        Ok(())
    }

    #[getter]
    fn samples(&self) -> usize {
        self.inner.samples
    }

    #[setter]
    fn set_samples(&mut self, value: usize) {
        self.inner.samples = value;
    }

    fn __repr__(&self) -> String {
        format!(
            "Globe(fill={}, samples={})",
            self.inner.fill, self.inner.samples
        )
    }
}

/// How a coat of hair is grown from particles in a two dimensional field.
///
/// ``count`` particles are dropped into the lattice and pushed around by it.
/// ``steps`` times ``step`` is how long each one is followed, and ``force``
/// over ``drag`` is roughly how fast it goes.
#[pyclass(name = "Hair", module = "perlin_noise")]
#[derive(Clone)]
pub struct PyHair {
    inner: Hair,
}

#[pymethods]
impl PyHair {
    #[new]
    #[pyo3(signature = (
        count=None,
        steps=None,
        step=None,
        force=None,
        drag=None,
        swirl=None,
        thickness=None,
        opacity=None,
        frizz=None,
        jitter=None,
        depth=None,
        taper=None,
        light=None,
        ambient=None,
        diffuse=None,
        specular=None,
        gloss=None,
        color_from_field=None,
        surface=None,
        palette=None,
        seed=None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        count: Option<usize>,
        steps: Option<usize>,
        step: Option<f64>,
        force: Option<f64>,
        drag: Option<f64>,
        swirl: Option<f64>,
        thickness: Option<f64>,
        opacity: Option<f64>,
        frizz: Option<f64>,
        jitter: Option<f64>,
        depth: Option<f64>,
        taper: Option<f64>,
        light: Option<Vec<f64>>,
        ambient: Option<f64>,
        diffuse: Option<f64>,
        specular: Option<f64>,
        gloss: Option<f64>,
        color_from_field: Option<bool>,
        surface: Option<PyRelief>,
        palette: Option<PyPalette>,
        seed: Option<u64>,
    ) -> PyResult<Self> {
        let mut inner = Hair::default();
        if let Some(count) = count {
            inner.count = count;
        }
        if let Some(steps) = steps {
            inner.steps = steps;
        }
        if let Some(step) = step {
            inner.step = step;
        }
        if let Some(force) = force {
            inner.force = force;
        }
        if let Some(drag) = drag {
            inner.drag = drag;
        }
        if let Some(swirl) = swirl {
            inner.swirl = swirl;
        }
        if let Some(thickness) = thickness {
            inner.thickness = thickness;
        }
        if let Some(opacity) = opacity {
            inner.opacity = opacity;
        }
        if let Some(frizz) = frizz {
            inner.frizz = frizz;
        }
        if let Some(jitter) = jitter {
            inner.jitter = jitter;
        }
        if let Some(depth) = depth {
            inner.depth = depth;
        }
        if let Some(taper) = taper {
            inner.taper = taper;
        }
        if let Some(light) = light {
            inner.light = pair2(light, "light")?;
        }
        if let Some(ambient) = ambient {
            inner.ambient = ambient;
        }
        if let Some(diffuse) = diffuse {
            inner.diffuse = diffuse;
        }
        if let Some(specular) = specular {
            inner.specular = specular;
        }
        if let Some(gloss) = gloss {
            inner.gloss = gloss;
        }
        if let Some(color_from_field) = color_from_field {
            inner.color_from_field = color_from_field;
        }
        if let Some(surface) = surface {
            inner.surface = surface.inner;
        }
        if let Some(palette) = palette {
            inner.palette = palette.into();
        }
        if let Some(seed) = seed {
            inner.seed = seed;
        }
        Ok(Self { inner })
    }

    #[getter]
    fn count(&self) -> usize {
        self.inner.count
    }

    #[setter]
    fn set_count(&mut self, value: usize) {
        self.inner.count = value;
    }

    #[getter]
    fn steps(&self) -> usize {
        self.inner.steps
    }

    #[setter]
    fn set_steps(&mut self, value: usize) {
        self.inner.steps = value;
    }

    #[getter]
    fn step(&self) -> f64 {
        self.inner.step
    }

    #[setter]
    fn set_step(&mut self, value: f64) {
        self.inner.step = value;
    }

    #[getter]
    fn force(&self) -> f64 {
        self.inner.force
    }

    #[setter]
    fn set_force(&mut self, value: f64) {
        self.inner.force = value;
    }

    #[getter]
    fn drag(&self) -> f64 {
        self.inner.drag
    }

    #[setter]
    fn set_drag(&mut self, value: f64) {
        self.inner.drag = value;
    }

    #[getter]
    fn swirl(&self) -> f64 {
        self.inner.swirl
    }

    #[setter]
    fn set_swirl(&mut self, value: f64) {
        self.inner.swirl = value;
    }

    #[getter]
    fn thickness(&self) -> f64 {
        self.inner.thickness
    }

    #[setter]
    fn set_thickness(&mut self, value: f64) {
        self.inner.thickness = value;
    }

    #[getter]
    fn opacity(&self) -> f64 {
        self.inner.opacity
    }

    #[setter]
    fn set_opacity(&mut self, value: f64) {
        self.inner.opacity = value;
    }

    #[getter]
    fn frizz(&self) -> f64 {
        self.inner.frizz
    }

    #[setter]
    fn set_frizz(&mut self, value: f64) {
        self.inner.frizz = value;
    }

    #[getter]
    fn jitter(&self) -> f64 {
        self.inner.jitter
    }

    #[setter]
    fn set_jitter(&mut self, value: f64) {
        self.inner.jitter = value;
    }

    #[getter]
    fn depth(&self) -> f64 {
        self.inner.depth
    }

    #[setter]
    fn set_depth(&mut self, value: f64) {
        self.inner.depth = value;
    }

    #[getter]
    fn taper(&self) -> f64 {
        self.inner.taper
    }

    #[setter]
    fn set_taper(&mut self, value: f64) {
        self.inner.taper = value;
    }

    #[getter]
    fn light(&self) -> (f64, f64) {
        let [x, y] = self.inner.light;
        (x, y)
    }

    #[setter]
    fn set_light(&mut self, value: Vec<f64>) -> PyResult<()> {
        self.inner.light = pair2(value, "light")?;
        Ok(())
    }

    #[getter]
    fn ambient(&self) -> f64 {
        self.inner.ambient
    }

    #[setter]
    fn set_ambient(&mut self, value: f64) {
        self.inner.ambient = value;
    }

    #[getter]
    fn diffuse(&self) -> f64 {
        self.inner.diffuse
    }

    #[setter]
    fn set_diffuse(&mut self, value: f64) {
        self.inner.diffuse = value;
    }

    #[getter]
    fn specular(&self) -> f64 {
        self.inner.specular
    }

    #[setter]
    fn set_specular(&mut self, value: f64) {
        self.inner.specular = value;
    }

    #[getter]
    fn gloss(&self) -> f64 {
        self.inner.gloss
    }

    #[setter]
    fn set_gloss(&mut self, value: f64) {
        self.inner.gloss = value;
    }

    #[getter]
    fn color_from_field(&self) -> bool {
        self.inner.color_from_field
    }

    #[setter]
    fn set_color_from_field(&mut self, value: bool) {
        self.inner.color_from_field = value;
    }

    #[getter]
    fn surface(&self) -> PyRelief {
        PyRelief::from_inner(self.inner.surface)
    }

    #[setter]
    fn set_surface(&mut self, value: PyRelief) {
        self.inner.surface = value.inner;
    }

    #[getter]
    fn palette(&self) -> PyPalette {
        self.inner.palette.into()
    }

    #[setter]
    fn set_palette(&mut self, value: PyPalette) {
        self.inner.palette = value.into();
    }

    #[getter]
    fn seed(&self) -> u64 {
        self.inner.seed
    }

    #[setter]
    fn set_seed(&mut self, value: u64) {
        self.inner.seed = value;
    }

    fn __repr__(&self) -> String {
        format!(
            "Hair(count={}, steps={}, color_from_field={})",
            self.inner.count, self.inner.steps, self.inner.color_from_field
        )
    }
}

/// A seeded Perlin noise field.
///
/// ``dims`` counts *cells* per axis, so ``[3, 1, 2]`` is a 3x1x2 grid with
/// 24 intersections. Every intersection holds a random unit vector derived
/// from ``seed``, so the same seed always rebuilds the same field.
///
/// Sample with :meth:`noise` for one point inside ``[0, dims[i]]`` on every
/// axis, or :meth:`table` for every intersection of a finer lattice.
#[pyclass(name = "Instance", module = "perlin_noise")]
#[derive(Clone)]
pub struct PyInstance {
    inner: Instance,
}

#[pymethods]
impl PyInstance {
    #[new]
    #[pyo3(signature = (dims, seed=1))]
    fn new(dims: Vec<usize>, seed: u64) -> PyResult<Self> {
        Ok(Self {
            inner: Instance::new(dims, seed).map_err(io_err)?,
        })
    }

    /// Cells per axis, as passed to the constructor.
    #[getter]
    fn dims(&self) -> Vec<usize> {
        self.inner.dims().to_vec()
    }

    /// Intersections per axis, one more than the cell count.
    #[getter]
    fn shape(&self) -> Vec<usize> {
        self.inner.shape().to_vec()
    }

    /// Total number of intersections.
    #[getter]
    fn intersections(&self) -> usize {
        self.inner.intersections()
    }

    /// The seed this field was built from.
    #[getter]
    fn seed(&self) -> u64 {
        self.inner.seed()
    }

    /// Gradient stored at an intersection, or ``None`` if ``index`` is out of
    /// the lattice.
    fn gradient(&self, index: Vec<usize>) -> Option<Vec<f64>> {
        self.inner.gradient(&index).map(|g| g.to_vec())
    }

    /// Noise at a single point.
    ///
    /// ``point`` must have one coordinate per dimension and lie within
    /// ``[0, dims[i]]`` on every axis. The result is 0 exactly at the
    /// intersections and stays within ``±sqrt(n)`` elsewhere.
    fn noise(&self, point: Vec<f64>) -> PyResult<f64> {
        self.inner.noise(&point).map_err(io_err)
    }

    /// The force the lattice applies at ``point``.
    ///
    /// This is the surrounding gradients blended under the same fade weights
    /// as :meth:`noise`, never longer than 1.
    fn force(&self, point: Vec<f64>) -> PyResult<Vec<f64>> {
        self.inner.force(&point).map_err(io_err)
    }

    /// Noise on a refined lattice.
    ///
    /// ``divisors`` holds one positive integer per dimension telling how many
    /// pieces each cell of that axis is cut into, so axis ``i`` ends up with
    /// ``dims[i] * divisors[i] + 1`` intersections.
    fn table(&self, divisors: Vec<usize>) -> PyResult<PyTable> {
        Ok(PyTable {
            inner: self.inner.table(&divisors).map_err(io_err)?,
        })
    }

    /// Intersections per axis that :meth:`table` would produce, without
    /// computing any noise.
    fn table_shape(&self, divisors: Vec<usize>) -> PyResult<Vec<usize>> {
        self.inner.table_shape(&divisors).map_err(io_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "Instance(dims={:?}, seed={})",
            self.inner.dims(),
            self.inner.seed()
        )
    }
}

/// Noise values sampled on a refined lattice, in row-major order.
#[pyclass(name = "Table", module = "perlin_noise")]
pub struct PyTable {
    inner: Table,
}

#[pymethods]
impl PyTable {
    /// Intersections per axis.
    #[getter]
    fn shape(&self) -> Vec<usize> {
        self.inner.shape().to_vec()
    }

    /// All values, row-major: the last axis varies fastest.
    #[getter]
    fn values(&self) -> Vec<f64> {
        self.inner.values().to_vec()
    }

    /// Value at a lattice index, or ``None`` if the index is out of the table.
    fn get(&self, index: Vec<usize>) -> Option<f64> {
        self.inner.get(&index)
    }

    fn __len__(&self) -> usize {
        self.inner.values().len()
    }

    fn __repr__(&self) -> String {
        format!("Table(shape={:?})", self.inner.shape())
    }
}

/// An 8-bit image, rows top to bottom, ``channels`` bytes per pixel.
#[pyclass(name = "Image", module = "perlin_noise")]
pub struct PyImage {
    inner: Image,
}

#[pymethods]
impl PyImage {
    /// Pixels across.
    #[getter]
    fn width(&self) -> usize {
        self.inner.width()
    }

    /// Pixels down.
    #[getter]
    fn height(&self) -> usize {
        self.inner.height()
    }

    /// Bytes per pixel: 1 for greyscale, 3 for colour.
    #[getter]
    fn channels(&self) -> usize {
        self.inner.channels()
    }

    /// The whole buffer, row-major.
    #[getter]
    fn pixels<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, self.inner.pixels())
    }

    /// One pixel's bytes, or ``None`` outside the image.
    fn pixel<'py>(&self, py: Python<'py>, x: usize, y: usize) -> Option<Bound<'py, PyBytes>> {
        self.inner.pixel(x, y).map(|bytes| PyBytes::new(py, bytes))
    }

    /// Write the image as a PNG.
    fn write_png(&self, path: PathBuf) -> PyResult<()> {
        self.inner.write_png(path).map_err(io_err)
    }

    fn __repr__(&self) -> String {
        format!(
            "Image({}x{}, channels={})",
            self.inner.width(),
            self.inner.height(),
            self.inner.channels()
        )
    }
}

/// Samples a noise field into pixels.
///
/// Wraps a field of two dimensions (greyscale or relief), three (colour) or
/// four (globe). Axes map to the image the way a row-major array does: the
/// first axis runs down the image, the second across it.
#[pyclass(name = "Renderer", module = "perlin_noise")]
pub struct PyRenderer {
    inner: Renderer,
}

#[pymethods]
impl PyRenderer {
    #[new]
    fn new(noise: PyInstance) -> PyResult<Self> {
        Ok(Self {
            inner: Renderer::new(noise.inner).map_err(io_err)?,
        })
    }

    /// The wrapped field, for sampling single points.
    fn instance(&self) -> PyInstance {
        PyInstance {
            inner: self.inner.instance().clone(),
        }
    }

    /// Greyscale image of a two dimensional field, one pixel per sample.
    fn grayscale(&self, divisors: Vec<usize>) -> PyResult<PyImage> {
        Ok(PyImage {
            inner: self.inner.grayscale(&divisors).map_err(io_err)?,
        })
    }

    /// Colour image of a two dimensional field, running each value through
    /// ``palette``.
    fn colored(&self, divisors: Vec<usize>, palette: PyPalette) -> PyResult<PyImage> {
        Ok(PyImage {
            inner: self
                .inner
                .colored(&divisors, palette.into())
                .map_err(io_err)?,
        })
    }

    /// Lit image of a two dimensional field, read as a height map.
    #[pyo3(signature = (divisors, relief=None))]
    fn relief(&self, divisors: Vec<usize>, relief: Option<PyRelief>) -> PyResult<PyImage> {
        let relief = relief.map(|r| r.inner).unwrap_or_default();
        Ok(PyImage {
            inner: self.inner.relief(&divisors, &relief).map_err(io_err)?,
        })
    }

    /// Colour image of a three dimensional field, taking red, green and blue
    /// from three evenly spaced slices of the third axis.
    fn rgb_channels(&self, divisors: Vec<usize>) -> PyResult<PyImage> {
        Ok(PyImage {
            inner: self.inner.rgb_channels(&divisors).map_err(io_err)?,
        })
    }

    /// Colour image of a three dimensional field, taking red, green and blue
    /// from three chosen slices of the third axis.
    fn rgb_channels_at(
        &self,
        divisors: Vec<usize>,
        slices: (usize, usize, usize),
    ) -> PyResult<PyImage> {
        Ok(PyImage {
            inner: self
                .inner
                .rgb_channels_at(&divisors, [slices.0, slices.1, slices.2])
                .map_err(io_err)?,
        })
    }

    /// Colour image of a three dimensional field, taking one slice of the
    /// third axis and running each value through ``palette``.
    fn rgb_palette(
        &self,
        divisors: Vec<usize>,
        slice: usize,
        palette: PyPalette,
    ) -> PyResult<PyImage> {
        Ok(PyImage {
            inner: self
                .inner
                .rgb_palette(&divisors, slice, palette.into())
                .map_err(io_err)?,
        })
    }

    /// A sphere carved out of a four dimensional field.
    #[pyo3(signature = (width, height, globe=None))]
    fn globe(&self, width: usize, height: usize, globe: Option<PyGlobe>) -> PyResult<PyImage> {
        let globe = globe.map(|g| g.inner).unwrap_or_default();
        Ok(PyImage {
            inner: self.inner.globe(width, height, &globe).map_err(io_err)?,
        })
    }

    /// A coat of hair drawn from particles set loose in a two dimensional
    /// field.
    #[pyo3(signature = (width, height, hair=None))]
    fn hair(&self, width: usize, height: usize, hair: Option<PyHair>) -> PyResult<PyImage> {
        let hair = hair.map(|h| h.inner).unwrap_or_default();
        Ok(PyImage {
            inner: self.inner.hair(width, height, &hair).map_err(io_err)?,
        })
    }

    fn __repr__(&self) -> String {
        let noise = self.inner.instance();
        format!("Renderer(dims={:?}, seed={})", noise.dims(), noise.seed())
    }
}

#[pymodule]
#[pyo3(name = "_native")]
fn python_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("MAX_DIMS", MAX_DIMS)?;
    m.add("MAX_SAMPLES", crate::MAX_SAMPLES)?;
    m.add_class::<PyPalette>()?;
    m.add_class::<PyShape>()?;
    m.add_class::<PyRelief>()?;
    m.add_class::<PyGlobe>()?;
    m.add_class::<PyHair>()?;
    m.add_class::<PyInstance>()?;
    m.add_class::<PyTable>()?;
    m.add_class::<PyImage>()?;
    m.add_class::<PyRenderer>()?;
    Ok(())
}
