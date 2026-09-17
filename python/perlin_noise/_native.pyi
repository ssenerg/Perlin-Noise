from collections.abc import Sequence
from os import PathLike

MAX_DIMS: int
MAX_SAMPLES: int

class Palette:
    """Maps a noise value to a colour.

    ``GRAY`` is black through white. ``BLUE_RED`` is blue through white to red.
    ``HEAT`` is black through red and orange to white. ``CRIMSON`` and
    ``AZURE`` are the ramps built for relief: they hold no black, so the dark
    comes only from the shading. ``TERRAIN`` is water, sand, grass, rock, snow.
    """

    GRAY: Palette
    BLUE_RED: Palette
    HEAT: Palette
    CRIMSON: Palette
    AZURE: Palette
    TERRAIN: Palette

    def color(self, position: float) -> tuple[int, int, int]:
        """Colour at ``position``, which is clamped to 0..1."""
        ...

class Shape:
    """How the noise is shaped into a height map before it is lit.

    ``SMOOTH`` is rolling hills. ``BILLOW`` turns the zero-crossings into
    creases between rounded blobs. ``RIDGED`` is billow upside down.
    """

    SMOOTH: Shape
    BILLOW: Shape
    RIDGED: Shape

class Relief:
    """Lighting and colour for a height map.

    The defaults are the glossy liquid look: billowing blobs, crimson, lit
    from the top left.
    """

    def __init__(
        self,
        shape: Shape | None = ...,
        palette: Palette | None = ...,
        light: Sequence[float] | None = ...,
        height: float | None = ...,
        gamma: float | None = ...,
        occlusion: float | None = ...,
        ambient: float | None = ...,
        diffuse: float | None = ...,
        specular: float | None = ...,
        gloss: float | None = ...,
    ) -> None: ...

    @property
    def shape(self) -> Shape: ...
    @shape.setter
    def shape(self, value: Shape) -> None: ...
    @property
    def palette(self) -> Palette: ...
    @palette.setter
    def palette(self, value: Palette) -> None: ...
    @property
    def light(self) -> tuple[float, float, float]: ...
    @light.setter
    def light(self, value: Sequence[float]) -> None: ...
    @property
    def height(self) -> float: ...
    @height.setter
    def height(self, value: float) -> None: ...
    @property
    def gamma(self) -> float: ...
    @gamma.setter
    def gamma(self, value: float) -> None: ...
    @property
    def occlusion(self) -> float: ...
    @occlusion.setter
    def occlusion(self, value: float) -> None: ...
    @property
    def ambient(self) -> float: ...
    @ambient.setter
    def ambient(self, value: float) -> None: ...
    @property
    def diffuse(self) -> float: ...
    @diffuse.setter
    def diffuse(self, value: float) -> None: ...
    @property
    def specular(self) -> float: ...
    @specular.setter
    def specular(self, value: float) -> None: ...
    @property
    def gloss(self) -> float: ...
    @gloss.setter
    def gloss(self, value: float) -> None: ...

class Globe:
    """Lighting and colour for a sphere carved out of a four dimensional field."""

    def __init__(
        self,
        surface: Relief | None = ...,
        rgb_from_fourth_axis: bool | None = ...,
        fill: float | None = ...,
        background: Sequence[int] | None = ...,
        samples: int | None = ...,
    ) -> None: ...

    @property
    def surface(self) -> Relief: ...
    @surface.setter
    def surface(self, value: Relief) -> None: ...
    @property
    def rgb_from_fourth_axis(self) -> bool: ...
    @rgb_from_fourth_axis.setter
    def rgb_from_fourth_axis(self, value: bool) -> None: ...
    @property
    def fill(self) -> float: ...
    @fill.setter
    def fill(self, value: float) -> None: ...
    @property
    def background(self) -> tuple[int, int, int]: ...
    @background.setter
    def background(self, value: Sequence[int]) -> None: ...
    @property
    def samples(self) -> int: ...
    @samples.setter
    def samples(self, value: int) -> None: ...

class Hair:
    """How a coat of hair is grown from particles in a two dimensional field.

    ``count`` particles are dropped into the lattice and pushed around by it.
    ``steps`` times ``step`` is how long each one is followed, and ``force``
    over ``drag`` is roughly how fast it goes.
    """

    def __init__(
        self,
        count: int | None = ...,
        steps: int | None = ...,
        step: float | None = ...,
        force: float | None = ...,
        drag: float | None = ...,
        swirl: float | None = ...,
        thickness: float | None = ...,
        opacity: float | None = ...,
        frizz: float | None = ...,
        jitter: float | None = ...,
        depth: float | None = ...,
        taper: float | None = ...,
        light: Sequence[float] | None = ...,
        ambient: float | None = ...,
        diffuse: float | None = ...,
        specular: float | None = ...,
        gloss: float | None = ...,
        color_from_field: bool | None = ...,
        surface: Relief | None = ...,
        palette: Palette | None = ...,
        seed: int | None = ...,
    ) -> None: ...

    @property
    def count(self) -> int: ...
    @count.setter
    def count(self, value: int) -> None: ...
    @property
    def steps(self) -> int: ...
    @steps.setter
    def steps(self, value: int) -> None: ...
    @property
    def step(self) -> float: ...
    @step.setter
    def step(self, value: float) -> None: ...
    @property
    def force(self) -> float: ...
    @force.setter
    def force(self, value: float) -> None: ...
    @property
    def drag(self) -> float: ...
    @drag.setter
    def drag(self, value: float) -> None: ...
    @property
    def swirl(self) -> float: ...
    @swirl.setter
    def swirl(self, value: float) -> None: ...
    @property
    def thickness(self) -> float: ...
    @thickness.setter
    def thickness(self, value: float) -> None: ...
    @property
    def opacity(self) -> float: ...
    @opacity.setter
    def opacity(self, value: float) -> None: ...
    @property
    def frizz(self) -> float: ...
    @frizz.setter
    def frizz(self, value: float) -> None: ...
    @property
    def jitter(self) -> float: ...
    @jitter.setter
    def jitter(self, value: float) -> None: ...
    @property
    def depth(self) -> float: ...
    @depth.setter
    def depth(self, value: float) -> None: ...
    @property
    def taper(self) -> float: ...
    @taper.setter
    def taper(self, value: float) -> None: ...
    @property
    def light(self) -> tuple[float, float]: ...
    @light.setter
    def light(self, value: Sequence[float]) -> None: ...
    @property
    def ambient(self) -> float: ...
    @ambient.setter
    def ambient(self, value: float) -> None: ...
    @property
    def diffuse(self) -> float: ...
    @diffuse.setter
    def diffuse(self, value: float) -> None: ...
    @property
    def specular(self) -> float: ...
    @specular.setter
    def specular(self, value: float) -> None: ...
    @property
    def gloss(self) -> float: ...
    @gloss.setter
    def gloss(self, value: float) -> None: ...
    @property
    def color_from_field(self) -> bool: ...
    @color_from_field.setter
    def color_from_field(self, value: bool) -> None: ...
    @property
    def surface(self) -> Relief: ...
    @surface.setter
    def surface(self, value: Relief) -> None: ...
    @property
    def palette(self) -> Palette: ...
    @palette.setter
    def palette(self, value: Palette) -> None: ...
    @property
    def seed(self) -> int: ...
    @seed.setter
    def seed(self, value: int) -> None: ...

class Instance:
    """A seeded Perlin noise field.

    ``dims`` counts *cells* per axis, so ``[3, 1, 2]`` is a 3x1x2 grid with
    24 intersections. Every intersection holds a random unit vector derived
    from ``seed``, so the same seed always rebuilds the same field.
    """

    def __init__(self, dims: Sequence[int], seed: int = 1) -> None: ...
    @property
    def dims(self) -> list[int]:
        """Cells per axis, as passed to the constructor."""
        ...
    @property
    def shape(self) -> list[int]:
        """Intersections per axis, one more than the cell count."""
        ...
    @property
    def intersections(self) -> int:
        """Total number of intersections."""
        ...
    @property
    def seed(self) -> int:
        """The seed this field was built from."""
        ...
    def gradient(self, index: Sequence[int]) -> list[float] | None:
        """Gradient stored at an intersection, or ``None`` if out of the lattice."""
        ...
    def noise(self, point: Sequence[float]) -> float:
        """Noise at a single point inside ``[0, dims[i]]`` on every axis."""
        ...
    def force(self, point: Sequence[float]) -> list[float]:
        """The force the lattice applies at ``point``, never longer than 1."""
        ...
    def table(self, divisors: Sequence[int]) -> Table:
        """Noise on a refined lattice, one divisor per dimension."""
        ...
    def table_shape(self, divisors: Sequence[int]) -> list[int]:
        """Intersections per axis that :meth:`table` would produce."""
        ...

class Table:
    """Noise values sampled on a refined lattice, in row-major order."""

    @property
    def shape(self) -> list[int]: ...
    @property
    def values(self) -> list[float]: ...
    def get(self, index: Sequence[int]) -> float | None: ...
    def __len__(self) -> int: ...

class Image:
    """An 8-bit image, rows top to bottom, ``channels`` bytes per pixel."""

    @property
    def width(self) -> int: ...
    @property
    def height(self) -> int: ...
    @property
    def channels(self) -> int: ...
    @property
    def pixels(self) -> bytes: ...
    def pixel(self, x: int, y: int) -> bytes | None:
        """One pixel's bytes, or ``None`` outside the image."""
        ...
    def write_png(self, path: str | PathLike[str]) -> None:
        """Write the image as a PNG."""
        ...

class Renderer:
    """Samples a noise field into pixels.

    Wraps a field of two dimensions (greyscale or relief), three (colour) or
    four (globe). The first axis runs down the image, the second across it.
    """

    def __init__(self, noise: Instance) -> None: ...
    def instance(self) -> Instance:
        """The wrapped field, for sampling single points."""
        ...
    def grayscale(self, divisors: Sequence[int]) -> Image:
        """Greyscale image of a two dimensional field."""
        ...
    def colored(self, divisors: Sequence[int], palette: Palette) -> Image:
        """Colour image of a two dimensional field through ``palette``."""
        ...
    def relief(self, divisors: Sequence[int], relief: Relief | None = ...) -> Image:
        """Lit image of a two dimensional field, read as a height map."""
        ...
    def rgb_channels(self, divisors: Sequence[int]) -> Image:
        """Red, green and blue from three evenly spaced slices of a 3D field."""
        ...
    def rgb_channels_at(
        self, divisors: Sequence[int], slices: tuple[int, int, int]
    ) -> Image:
        """Red, green and blue from three chosen slices of a 3D field."""
        ...
    def rgb_palette(
        self, divisors: Sequence[int], slice: int, palette: Palette
    ) -> Image:
        """One slice of a 3D field, coloured through ``palette``."""
        ...
    def globe(self, width: int, height: int, globe: Globe | None = ...) -> Image:
        """A sphere carved out of a four dimensional field."""
        ...
    def hair(self, width: int, height: int, hair: Hair | None = ...) -> Image:
        """A coat of hair from particles set loose in a two dimensional field."""
        ...
