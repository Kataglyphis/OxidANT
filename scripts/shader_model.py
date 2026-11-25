"""A Python model of OxidANT's shared Slang modules, for the f64 references of shaders/tests/vectors/.

Each function mirrors its Slang twin with every literal at its f32 value. Every operation rounds as the current
`evaluation` says: not at all for the references, to f32 to tell how far an f32 kernel may stray from them.
"""
import functools
import math
import re
import struct
from collections.abc import Callable, Generator, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import ClassVar

M32 = 0xFFFFFFFF


def f32(x: float) -> float:
    return struct.unpack("<f", struct.pack("<f", x))[0]


def shortest_f32(x: float) -> float:
    """The shortest decimal that parses back to the f32 `x`, as both runners parse it."""
    for digits in range(1, 10):
        candidate = float(f"{x:.{digits}g}")
        if f32(candidate) == x:
            return candidate
    raise ValueError(x)


class Model:
    """The rounding every operation takes, and what the current evaluation recorded on its way."""

    rounding: Callable[[float], float] = float
    margins: ClassVar[list[tuple[str, float]]] = []
    visits: ClassVar[set[str]] = set()


@contextmanager
def evaluation(rounding: Callable[[float], float]) -> Generator[None]:
    Model.rounding, Model.margins, Model.visits = rounding, [], set()
    try:
        yield
    finally:
        Model.rounding = float


def visit(label: str) -> None:
    Model.visits.add(label)


class Real:
    """A float whose every operation rounds as the current evaluation does."""

    __slots__ = ("v",)

    def __init__(self, v: float) -> None:
        self.v = Model.rounding(v)

    def __add__(self, o: "Num") -> "Real":
        return Real(self.v + val(o))

    def __radd__(self, o: "Num") -> "Real":
        return Real(val(o) + self.v)

    def __sub__(self, o: "Num") -> "Real":
        return Real(self.v - val(o))

    def __rsub__(self, o: "Num") -> "Real":
        return Real(val(o) - self.v)

    def __mul__(self, o: "Num") -> "Real":
        return Real(self.v * val(o))

    def __rmul__(self, o: "Num") -> "Real":
        return Real(val(o) * self.v)

    def __truediv__(self, o: "Num") -> "Real":
        return Real(self.v / val(o))

    def __rtruediv__(self, o: "Num") -> "Real":
        return Real(val(o) / self.v)

    def __neg__(self) -> "Real":
        return Real(-self.v)


Num = Real | float | int


def val(o: Num) -> float:
    """A Real's value; a bare float is a Slang `float` literal, so it takes its f32 value."""
    if isinstance(o, Real):
        return o.v
    return f32(o) if isinstance(o, float) else float(o)


def real(o: Num) -> Real:
    return o if isinstance(o, Real) else Real(val(o))


class Vec:
    """A float vector with the componentwise arithmetic and the swizzles Slang has."""

    __slots__ = ("c",)

    def __init__(self, *c: Num) -> None:
        self.c = tuple(real(x) for x in c)

    @property
    def x(self) -> Real:
        return self.c[0]

    @property
    def y(self) -> Real:
        return self.c[1]

    @property
    def z(self) -> Real:
        return self.c[2]

    @property
    def w(self) -> Real:
        return self.c[3]

    def __getattr__(self, name: str) -> "Vec":
        if len(name) < 2 or any(ch not in "xyzw" for ch in name):
            raise AttributeError(name)
        return Vec(*(self.c["xyzw".index(ch)] for ch in name))

    def zip(self, o: "Vec | Num", op: Callable[[Real, Num], Real]) -> "Vec":
        if isinstance(o, Vec):
            return Vec(*(op(a, b) for a, b in zip(self.c, o.c, strict=True)))
        return Vec(*(op(a, o) for a in self.c))

    def __add__(self, o: "Vec | Num") -> "Vec":
        return self.zip(o, lambda a, b: a + b)

    def __radd__(self, o: Num) -> "Vec":
        return self.zip(o, lambda a, b: b + a)

    def __sub__(self, o: "Vec | Num") -> "Vec":
        return self.zip(o, lambda a, b: a - b)

    def __rsub__(self, o: Num) -> "Vec":
        return self.zip(o, lambda a, b: b - a)

    def __mul__(self, o: "Vec | Num") -> "Vec":
        return self.zip(o, lambda a, b: a * b)

    def __rmul__(self, o: Num) -> "Vec":
        return self.zip(o, lambda a, b: b * a)

    def __truediv__(self, o: "Vec | Num") -> "Vec":
        return self.zip(o, lambda a, b: a / b)

    def __neg__(self) -> "Vec":
        return Vec(*(-a for a in self.c))


def each(f: Callable[[Real], Real], v: Vec) -> Vec:
    return Vec(*(f(a) for a in v.c))


def rmax(a: Num, b: Num) -> Real:
    return Real(max(val(a), val(b)))


def rmin(a: Num, b: Num) -> Real:
    return Real(min(val(a), val(b)))


def clamp(x: Num, lo: Num, hi: Num) -> Real:
    return rmin(rmax(x, lo), hi)


def sqrt(x: Real) -> Real:
    return Real(math.sqrt(x.v))


def pow_(x: Real, y: Num) -> Real:
    return Real(math.pow(x.v, val(y)))


def sin(x: Real) -> Real:
    return Real(math.sin(x.v))


def cos(x: Real) -> Real:
    return Real(math.cos(x.v))


def floor(x: Real) -> Real:
    return Real(math.floor(x.v))


def below(a: Num, b: Num, label: str) -> bool:
    """`a < b`, recording how far a device's rounding would have to move either side to flip it."""
    a_v, b_v = val(a), val(b)
    Model.margins.append((label, abs(a_v - b_v) / max(1.0, abs(a_v), abs(b_v))))
    return a_v < b_v


def dot(a: Vec, b: Vec) -> Real:
    total = a.c[0] * b.c[0]
    for x, y in zip(a.c[1:], b.c[1:], strict=True):
        total = total + x * y
    return total


def normalize(v: Vec) -> Vec:
    return v / sqrt(dot(v, v))


def cross(a: Vec, b: Vec) -> Vec:
    return Vec(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x)


def reflect(i: Vec, n: Vec) -> Vec:
    return i - n * (2.0 * dot(n, i))


def vec3(c: Sequence[Num]) -> Vec:
    return Vec(*c)


def saturate(x: Real) -> Real:
    return clamp(x, 0.0, 1.0)


# ---- common/brdf_energy_fit.slang: generated, so its coefficients are read and its evaluation mirrored ----

FIT_SOURCE = Path(__file__).resolve().parents[1] / "shaders" / "common" / "brdf_energy_fit.slang"


@functools.cache
def energy_fit() -> tuple[tuple[tuple[float, float], ...], ...]:
    """GGX_ENERGY_COEFFICIENTS' float2 literals, one row per basis function of the first coordinate."""
    table = FIT_SOURCE.read_text(encoding="utf-8").split("GGX_ENERGY_COEFFICIENTS[", 1)[1].split("};", 1)[0]
    pairs = [(float(a), float(b)) for a, b in re.findall(r"float2\(([-+.\deE]+), ([-+.\deE]+)\)", table)]
    size = math.isqrt(len(pairs))
    if size == 0 or size * size != len(pairs):
        raise ValueError(f"{FIT_SOURCE.name}: {len(pairs)} coefficients are not a square")
    return tuple(tuple(pairs[i * size:(i + 1) * size]) for i in range(size))


def ggx_energy_weights(f: Real) -> list[Real]:
    g = 1.0 - f
    return [g * g * g / 6.0, (4.0 - 6.0 * f * f + 3.0 * f * f * f) / 6.0,
            (4.0 - 6.0 * g * g + 3.0 * g * g * g) / 6.0, f * f * f / 6.0]


def ggx_energy_fit(t: Vec) -> Vec:
    rows = energy_fit()
    cells = float(len(rows) - 3)
    x = t * cells + 1.0
    cell_x, cell_y = clamp(floor(x.x), 1.0, cells), clamp(floor(x.y), 1.0, cells)
    wi, wj = ggx_energy_weights(x.x - cell_x), ggx_energy_weights(x.y - cell_y)
    first_i, first_j = int(cell_x.v) - 1, int(cell_y.v) - 1
    e = Vec(0.0, 0.0)
    for i in range(4):
        row = Vec(0.0, 0.0)
        for j in range(4):
            row = row + Vec(*rows[first_i + i][first_j + j]) * wj[j]
        e = e + row * wi[i]
    return e


# ---- common/constants.slang ----

PI = 3.14159265358979323846
MIN_ROUGHNESS = 0.045


# ---- common/brdf.slang ----


def lambert_diffuse(albedo: Vec) -> Vec:
    return albedo / PI


def min_roughness(roughness: Real) -> Real:
    if roughness.v < f32(MIN_ROUGHNESS):
        visit("roughness floor")
    return rmax(roughness, MIN_ROUGHNESS)


def distribution_ggx(n_dot_h: Real, roughness: Real) -> Real:
    r = min_roughness(roughness)
    a = r * r
    a2 = a * a
    denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0
    return a2 / (PI * rmax(denom * denom, 1e-20))


def smith_k_analytic(roughness: Real) -> Real:
    r = min_roughness(roughness) + 1.0
    return (r * r) / 8.0


def smith_k_sampled(roughness: Real) -> Real:
    r = min_roughness(roughness)
    return r * r / 2.0


def geometry_smith_k(n_dot_v: Real, n_dot_l: Real, k: Real) -> Real:
    g1 = n_dot_v / rmax(n_dot_v * (1.0 - k) + k, 1e-6)
    g2 = n_dot_l / rmax(n_dot_l * (1.0 - k) + k, 1e-6)
    return g1 * g2


def fresnel_schlick(cos_theta: Real, f0: Vec) -> Vec:
    if not 0.0 <= 1.0 - cos_theta.v <= 1.0:
        visit("fresnel clamp")
    return f0 + (1.0 - f0) * pow_(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0)


@dataclass
class Surface:
    n: Vec
    v: Vec
    l: Vec
    albedo: Vec
    metallic: Real
    roughness: Real
    f0: Vec


def ggx_single_scatter(n_dot_v: Real, n_dot_l: Real, n_dot_h: Real, h_dot_v: Real, roughness: Real, f0: Vec,
                       k: Real) -> Vec:
    d = distribution_ggx(n_dot_h, roughness)
    g = geometry_smith_k(n_dot_v, n_dot_l, k)
    f = fresnel_schlick(h_dot_v, f0)
    return f * (d * g) / rmax(4.0 * n_dot_v * n_dot_l, 1e-6)


GGX_ENERGY_MIN_N_DOT_V = 1e-4


def energy_low() -> Vec:
    return Vec(pow_(real(GGX_ENERGY_MIN_N_DOT_V), 1.0 / 3.0), sqrt(real(MIN_ROUGHNESS)))


def ggx_energy_node(t: Vec) -> Vec:
    if not all(0.0 <= a.v <= 1.0 for a in t.c):
        visit("fit coordinate clamp")
    low = energy_low()
    c = low + each(lambda a: clamp(a, 0.0, 1.0), t) * (1.0 - low)
    return Vec(c.x * c.x * c.x, c.y * c.y)


def ggx_energy_coordinates(n_dot_v: Real, roughness: Real) -> Vec:
    if not f32(GGX_ENERGY_MIN_N_DOT_V) <= n_dot_v.v <= 1.0 or not f32(MIN_ROUGHNESS) <= roughness.v <= 1.0:
        visit("energy input clamp")
    low = energy_low()
    c = Vec(pow_(clamp(n_dot_v, GGX_ENERGY_MIN_N_DOT_V, 1.0), 1.0 / 3.0), sqrt(clamp(roughness, MIN_ROUGHNESS, 1.0)))
    return (c - low) / (1.0 - low)


def ggx_energy(n_dot_v: Real, roughness: Real) -> Vec:
    return ggx_energy_fit(ggx_energy_coordinates(n_dot_v, roughness))


def multiscatter_gain(energy: Vec, f0: Vec) -> Vec:
    if (energy.x + energy.y).v < f32(1e-3):
        visit("gain floor")
    return 1.0 + f0 * (1.0 / rmax(energy.x + energy.y, 1e-3) - 1.0)


def specular_albedo(energy: Vec, f0: Vec) -> Vec:
    return multiscatter_gain(energy, f0) * (f0 * energy.x + energy.y)


def brdf_smith_k(s: Surface, radiance: Vec, k: Real) -> Vec:
    h = normalize(s.l + s.v)
    if dot(s.n, s.l).v <= 0.0:
        visit("light below the horizon")
    n_dot_l = rmax(dot(s.n, s.l), 0.0)
    n_dot_v = rmax(dot(s.n, s.v), 1e-4)
    n_dot_h = rmax(dot(s.n, h), 0.0)
    h_dot_v = rmax(dot(h, s.v), 0.0)
    r = min_roughness(s.roughness)
    energy = ggx_energy(n_dot_v, r)
    single = ggx_single_scatter(n_dot_v, n_dot_l, n_dot_h, h_dot_v, r, s.f0, k)
    specular = multiscatter_gain(energy, s.f0) * single
    diffuse = each(saturate, 1.0 - specular_albedo(energy, s.f0)) * (1.0 - s.metallic) * lambert_diffuse(s.albedo)
    return (diffuse + specular) * radiance * n_dot_l


def brdf_direct(s: Surface, radiance: Vec) -> Vec:
    return brdf_smith_k(s, radiance, smith_k_analytic(s.roughness))


def brdf_sampled(s: Surface) -> Vec:
    return brdf_smith_k(s, Vec(1.0, 1.0, 1.0), smith_k_sampled(s.roughness))


# ---- common/noise.slang ----


def mod289(x: Vec) -> Vec:
    return x - each(floor, x * (1.0 / 289.0)) * 289.0


def permute(x: Vec) -> Vec:
    return mod289(((x * 34.0) + 10.0) * x)


def taylor_inv_sqrt(r: Vec) -> Vec:
    return 1.79284291400159 - 0.85373472095314 * r


def step(edge: Vec, x: Vec) -> Vec:
    return Vec(*(1.0 if a.v >= e.v else 0.0 for e, a in zip(edge.c, x.c, strict=True)))


def simplex_gradients(p: Vec) -> tuple[Vec, Vec, Vec, Vec]:
    """snoise's 7x7 gradient grid over an octahedron, for the four corners' permutations `p`."""
    n_ = real(0.142857142857)
    d = Vec(0.0, 0.5, 1.0, 2.0)
    ns = d.wyz * n_ - d.xzx
    j = p - 49.0 * each(floor, p * ns.z * ns.z)
    x_ = each(floor, j * ns.z)
    y_ = each(floor, j - 7.0 * x_)
    x = x_ * ns.x + ns.yyyy
    y = y_ * ns.x + ns.yyyy
    h = 1.0 - each(lambda a: Real(abs(a.v)), x) - each(lambda a: Real(abs(a.v)), y)
    b0 = Vec(x.x, x.y, y.x, y.y)
    b1 = Vec(x.z, x.w, y.z, y.w)
    s0 = each(floor, b0) * 2.0 + 1.0
    s1 = each(floor, b1) * 2.0 + 1.0
    sh = -Vec(*(1.0 if a.v <= 0.0 else 0.0 for a in h.c))
    a0 = b0.xzyw + s0.xzyw * sh.xxyy
    a1 = b1.xzyw + s1.xzyw * sh.zzww
    p0, p1, p2, p3 = Vec(a0.x, a0.y, h.x), Vec(a0.z, a0.w, h.y), Vec(a1.x, a1.y, h.z), Vec(a1.z, a1.w, h.w)
    norm = taylor_inv_sqrt(Vec(dot(p0, p0), dot(p1, p1), dot(p2, p2), dot(p3, p3)))
    return p0 * norm.x, p1 * norm.y, p2 * norm.z, p3 * norm.w


def snoise(v: Vec) -> Real:
    c = Vec(1.0 / 6.0, 1.0 / 3.0)
    i = each(floor, v + dot(v, c.yyy))
    x0 = v - i + dot(i, c.xxx)
    g = step(x0.yzx, x0.xyz)
    l = 1.0 - g
    i1 = Vec(*(rmin(a, b) for a, b in zip(g.c, l.zxy.c, strict=True)))
    i2 = Vec(*(rmax(a, b) for a, b in zip(g.c, l.zxy.c, strict=True)))
    x1 = x0 - i1 + c.xxx
    x2 = x0 - i2 + c.yyy
    x3 = x0 - 0.5
    i = mod289(i)
    p = permute(Vec(0.0, i1.z, i2.z, 1.0) + i.z)
    p = permute(p + i.y + Vec(0.0, i1.y, i2.y, 1.0))
    p = permute(p + i.x + Vec(0.0, i1.x, i2.x, 1.0))
    p0, p1, p2, p3 = simplex_gradients(p)
    m = Vec(*(rmax(a, 0.0) for a in (0.5 - Vec(dot(x0, x0), dot(x1, x1), dot(x2, x2), dot(x3, x3))).c))
    m = m * m
    return 105.0 * dot(m * m, Vec(dot(p0, x0), dot(p1, x1), dot(p2, x2), dot(p3, x3)))


def fbm(p: Vec, freq: Real, octaves: int) -> Real:
    gain = Real(2.0 ** val(-0.5))
    amp = real(1.0)
    noise = real(0.0)
    if octaves <= 0:
        visit("no octave")
    for _ in range(octaves):
        noise = noise + amp * snoise(p * freq)
        freq = freq * 2.0
        amp = amp * gain
    return noise


# ---- common/path_estimator.slang ----


def path_rng_seed(pixel_index: int, stream: int) -> int:
    return (pixel_index + stream * 2654435769) & M32


def rng_next(state: int) -> tuple[Real, int]:
    state = (state * 747796405 + 1) & M32
    word = (((state >> ((state >> 28) + 4)) ^ state) * 277803737) & M32
    word = (word >> 22) ^ word
    return Real(float(word)) / 4294967295.0, state


def russian_roulette_survives(bounce: int, throughput: Vec, rng: int) -> tuple[bool, Vec, int]:
    if bounce < 3:
        visit("roulette not yet")
        return True, throughput, rng
    survival = clamp(rmax(throughput.x, rmax(throughput.y, throughput.z)), 0.05, 0.95)
    drawn, rng = rng_next(rng)
    if below(survival, drawn, "roulette"):
        visit("roulette ends the path")
        return False, throughput, rng
    visit("roulette keeps the path")
    return True, throughput / survival, rng


def orthonormal_basis(n: Vec) -> tuple[Vec, Vec]:
    s = real(1.0 if n.z.v >= 0.0 else -1.0)
    a = -1.0 / (s + n.z)
    c = n.x * n.y * a
    return Vec(1.0 + s * n.x * n.x * a, s * c, -s * n.x), Vec(c, s + n.y * n.y * a, -n.y)


def sample_ggx_visible_normal(v: Vec, alpha: Real, u: Vec) -> Vec:
    vh = normalize(Vec(alpha * v.x, alpha * v.y, v.z))
    length_squared = vh.x * vh.x + vh.y * vh.y
    if length_squared.v > 0.0:
        t1 = Vec(-vh.y, vh.x, 0.0) / sqrt(length_squared)
    else:
        visit("view along the normal")
        t1 = Vec(1.0, 0.0, 0.0)
    t2 = cross(vh, t1)
    r = sqrt(u.x)
    phi = 2.0 * real(PI) * u.y
    p1 = r * cos(phi)
    p2 = r * sin(phi)
    s = 0.5 * (1.0 + vh.z)
    p2 = (1.0 - s) * sqrt(rmax(1.0 - p1 * p1, 0.0)) + s * p2
    nh = t1 * p1 + t2 * p2 + vh * sqrt(rmax(1.0 - p1 * p1 - p2 * p2, 0.0))
    return normalize(Vec(alpha * nh.x, alpha * nh.y, rmax(nh.z, 1e-6)))


def ggx_visible_normal_pdf(n_dot_v: Real, n_dot_h: Real, alpha: Real) -> Real:
    a2 = alpha * alpha
    d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0
    ndf = a2 / (PI * d * d)
    g1 = 2.0 * n_dot_v / (n_dot_v + sqrt(a2 + (1.0 - a2) * n_dot_v * n_dot_v))
    return ndf * g1 / (4.0 * n_dot_v)


def specular_lobe_probability(albedo: Vec, metallic: Real, f0: Vec, n_dot_v: Real) -> Real:
    luma = Vec(0.2126, 0.7152, 0.0722)
    fresnel = fresnel_schlick(n_dot_v, f0)
    specular = dot(fresnel, luma)
    diffuse = dot((1.0 - fresnel) * (1.0 - metallic) * albedo, luma)
    return clamp(specular / rmax(specular + diffuse, 1e-6), 0.1, 1.0)


def bounce_direction(s: Surface, specular: Real, rng: int) -> tuple[Vec, int]:
    t, b = orthonormal_basis(s.n)
    lobe, rng = rng_next(rng)
    if below(lobe, specular, "lobe"):
        visit("specular lobe")
        local = Vec(dot(s.v, t), dot(s.v, b), dot(s.v, s.n))
        u0, rng = rng_next(rng)
        u1, rng = rng_next(rng)
        h = sample_ggx_visible_normal(local, s.roughness * s.roughness, Vec(u0, u1))
        return reflect(-s.v, t * h.x + b * h.y + s.n * h.z), rng
    visit("diffuse lobe")
    u0, rng = rng_next(rng)
    theta = 2.0 * real(PI) * u0
    u1, rng = rng_next(rng)
    z = 2.0 * u1 - 1.0
    r = sqrt(rmax(1.0 - z * z, 0.0))
    direction = s.n + Vec(r * cos(theta), r * sin(theta), z)
    return (s.n if below(dot(direction, direction), 1e-8, "lobe tip") else direction), rng


def sample_bounce(s: Surface, face_normal: Vec, rng: int) -> tuple[Vec, Vec, int]:
    """`s.l` is unused and `s.f0` is overwritten: the bounce derives f0 from the albedo itself."""
    s.f0 = Vec(0.04, 0.04, 0.04) + (s.albedo - Vec(0.04, 0.04, 0.04)) * s.metallic
    n_dot_v = rmax(dot(s.n, s.v), 1e-4)
    specular = specular_lobe_probability(s.albedo, s.metallic, s.f0, n_dot_v)
    direction, rng = bounce_direction(s, specular, rng)
    s.l = normalize(direction)
    n_dot_l = dot(s.n, s.l)
    if not below(0.0, n_dot_l, "below the surface") or not below(0.0, dot(face_normal, s.l), "below the face"):
        visit("the path ends below a surface")
        return s.l, Vec(0.0, 0.0, 0.0), rng
    n_dot_h = rmax(dot(s.n, normalize(s.v + s.l)), 0.0)
    pdf = specular * ggx_visible_normal_pdf(n_dot_v, n_dot_h, s.roughness * s.roughness)
    pdf = pdf + (1.0 - specular) * n_dot_l / PI
    return s.l, brdf_sampled(s) / rmax(pdf, 1e-6), rng


# ---- common/hdr_safe.slang ----

HDR_MAX = 65000.0


def bits_to_f32(word: int) -> float:
    return struct.unpack("<f", struct.pack("<I", word & M32))[0]


def hdr_safe(words: Sequence[int]) -> Vec:
    """The radiance arrives as its bits, as the kernel reads it, so NaN and the infinities can be inputs."""
    out: list[Real] = []
    for word in words:
        x = bits_to_f32(word)
        if math.isnan(x):
            visit("NaN zeroed")
            out.append(Real(0.0))
            continue
        if x < 0.0:
            visit("negative zeroed")
        if x > f32(HDR_MAX):
            visit("infinity capped" if math.isinf(x) else "capped at HDR_MAX")
        out.append(clamp(Real(x), 0.0, HDR_MAX))
    return Vec(*out)


# ---- common/aces.slang ----

ACES_A, ACES_B, ACES_C, ACES_D, ACES_E = 2.51, 0.03, 2.43, 0.59, 0.14


def unit_clamp(v: Vec, label: str) -> Vec:
    if any(not 0.0 <= a.v <= 1.0 for a in v.c):
        visit(label)
    return each(lambda a: clamp(a, 0.0, 1.0), v)


def aces_tonemap(x: Vec) -> Vec:
    return unit_clamp((x * (ACES_A * x + ACES_B)) / (x * (ACES_C * x + ACES_D) + ACES_E), "tonemap clamp")


def aces_tonemap_inverse(display: Vec) -> Vec:
    y = unit_clamp(display, "display clamp")
    qa = ACES_A - ACES_C * y
    qb = ACES_B - ACES_D * y
    return (-qb + each(sqrt, qb * qb + 4.0 * qa * ACES_E * y)) / (2.0 * qa)


def linear_to_srgb(c: Vec) -> Vec:
    out: list[Real] = []
    for x in c.c:
        if below(x, 0.0031308, "srgb segment"):
            visit("linear segment")
            out.append(x * 12.92)
        else:
            visit("power segment")
            out.append(1.055 * pow_(rmax(x, 0.0), 1.0 / 2.4) - 0.055)
    return Vec(*out)


# ---- common/fullscreen.slang: the WGSL branch; the kernel writes only what both targets share ----


def fullscreen_vs(vid: int) -> tuple[Vec, Vec]:
    x = Real(float((vid & M32) // 2)) * 4.0 - 1.0
    y = Real(float((vid & M32) % 2)) * 4.0 - 1.0
    return Vec(x, y, 0.0, 1.0), Vec((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5)


def fullscreen_uv_to_ndc(uv: Vec) -> Vec:
    return Vec(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0)


def fullscreen_ndc_to_uv(ndc: Vec) -> Vec:
    return Vec(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5)


# ---- common/camera_ray.slang ----


def mul_mat_vec(rows: Sequence[Vec], v: Vec) -> Vec:
    return Vec(*(dot(row, v) for row in rows))


def camera_ray(clip_to_world: Sequence[Vec], ndc: Vec) -> tuple[Vec, Vec, Real]:
    near = mul_mat_vec(clip_to_world, Vec(ndc.x, ndc.y, 0.0, 1.0))
    far = mul_mat_vec(clip_to_world, Vec(ndc.x, ndc.y, 1.0, 1.0))
    origin = near.xyz / near.w
    span = far.xyz / far.w - origin
    return origin, normalize(span), sqrt(dot(span, span))


# ---- common/sky_model.slang ----

SKY_ZENITH = (0.09, 0.16, 0.35)
SKY_HORIZON = (0.55, 0.62, 0.72)
SKY_GROUND = (0.18, 0.16, 0.15)


def lerp(a: Vec, b: Vec, t: Num) -> Vec:
    """SPIR-V's FMix and WGSL's mix: a * (1 - t) + b * t."""
    return a * (1.0 - real(t)) + b * real(t)


def sky_gradient(d: Vec) -> Vec:
    if below(d.y, 0.0, "sky gradient's horizon"):
        visit("ground side")
        return lerp(vec3(SKY_HORIZON), vec3(SKY_GROUND), clamp(-d.y * 3.0, 0.0, 1.0))
    visit("sky side")
    return lerp(vec3(SKY_HORIZON), vec3(SKY_ZENITH), pow_(clamp(d.y, 0.0, 1.0), 0.7))


def hemisphere_irradiance(n: Vec) -> Vec:
    sky = lerp(vec3(SKY_HORIZON), vec3(SKY_ZENITH), pow_(clamp(n.y, 0.0, 1.0), 0.7))
    return lerp(vec3(SKY_GROUND) * 0.7, sky, clamp(n.y * 0.5 + 0.5, 0.0, 1.0))


# ---- common/ambient.slang ----


def ambient_split_sum(irradiance: Vec, reflection: Vec, albedo: Vec, metallic: Real, f0: Vec, energy: Vec) -> Vec:
    spec = specular_albedo(energy, f0)
    diffuse = each(saturate, 1.0 - spec) * (1.0 - metallic) * albedo * irradiance
    return diffuse + reflection * spec


def analytic_ambient(s: Surface, energy: Vec) -> Vec:
    irradiance = hemisphere_irradiance(s.n)
    reflection = lerp(sky_gradient(reflect(-s.v, s.n)), irradiance, s.roughness * s.roughness)
    return ambient_split_sum(irradiance, reflection, s.albedo, s.metallic, s.f0, energy)


# ---- common/gltf_material.slang ----

ALPHA_MASK = 1


def transform_uv(uv: Vec, row0: Vec, row1: Vec) -> Vec:
    p = Vec(uv.x, uv.y, 1.0)
    return Vec(dot(p, row0), dot(p, row1))


def normalize_or(v: Vec, fallback: Vec) -> Vec:
    if dot(v, v).v > f32(1e-30):
        return normalize(v)
    visit("zero vector")
    return fallback


def tangent_space_normal(texel: Vec, scale: Real) -> Vec:
    n = texel * 2.0 - 1.0
    z = n.z
    if texel.z.v == 0.0:
        visit("two-channel texel")
        z = sqrt(saturate(1.0 - dot(n.xy, n.xy)))
    return Vec(n.x * scale, n.y * scale, z)


def apply_normal_map(normal: Vec, tangent: Vec, tangent_normal: Vec, front_face: bool) -> Vec:
    t = normalize_or(tangent.xyz - normal * dot(normal, tangent.xyz), Vec(0.0, 0.0, 0.0))
    b = cross(normal, t) * tangent.w
    mapped = normalize_or(t * tangent_normal.x + b * tangent_normal.y + normal * tangent_normal.z, normal)
    if front_face:
        return mapped
    visit("back face")
    return -mapped


def mirror_sign(m: Sequence[Vec]) -> Real:
    if below(dot(m[0], cross(m[1], m[2])), 0.0, "determinant sign"):
        visit("mirrored")
        return Real(-1.0)
    return Real(1.0)


def normal_transform(m: Sequence[Vec]) -> list[Vec]:
    sign = mirror_sign(m)
    return [cross(m[1], m[2]) * sign, cross(m[2], m[0]) * sign, cross(m[0], m[1]) * sign]


def material_metallic(factor: Real, sampled: Real) -> Real:
    x = factor * sampled
    if not 0.0 <= x.v <= 1.0:
        visit("metallic clamp")
    return clamp(x, 0.0, 1.0)


def material_roughness(factor: Real, sampled: Real) -> Real:
    x = factor * sampled
    if x.v < f32(MIN_ROUGHNESS):
        visit("roughness floor")
    return clamp(x, MIN_ROUGHNESS, 1.0)


def alpha_masked_out(alpha_mode: int, alpha: Real, cutoff: Real) -> bool:
    if alpha_mode != ALPHA_MASK:
        visit("not MASK")
        return False
    if below(alpha, cutoff, "alpha against its cutoff"):
        visit("cut out")
        return True
    visit("kept")
    return False


def material_occlusion(strength: Real, sampled: Real) -> Real:
    return 1.0 + (sampled - 1.0) * strength
