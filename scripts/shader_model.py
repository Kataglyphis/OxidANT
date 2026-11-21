"""A Python model of OxidANT's shared Slang modules, for the f64 references of shaders/tests/vectors/.

Each function mirrors its Slang twin with every literal at its f32 value. Every operation rounds as the current
`evaluation` says: not at all for the references, to f32 to tell how far an f32 kernel may stray from them.
"""
import math
import struct
from collections.abc import Callable, Generator, Sequence
from contextlib import contextmanager
from dataclasses import dataclass
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


# ---- common/brdf.slang ----

PI = 3.14159265358979323846


def lambert_diffuse(albedo: Vec) -> Vec:
    return albedo / PI


def distribution_ggx(n_dot_h: Real, roughness: Real) -> Real:
    if roughness.v < f32(0.045):
        visit("roughness floor")
    r = rmax(roughness, 0.045)
    a = r * r
    a2 = a * a
    denom = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0
    return a2 / (PI * rmax(denom * denom, 1e-20))


def smith_k_analytic(roughness: Real) -> Real:
    r = roughness + 1.0
    return (r * r) / 8.0


def smith_k_sampled(roughness: Real) -> Real:
    return roughness * roughness / 2.0


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


def brdf_smith_k(s: Surface, radiance: Vec, k: Real) -> Vec:
    h = normalize(s.l + s.v)
    if dot(s.n, s.l).v <= 0.0:
        visit("light below the horizon")
    n_dot_l = rmax(dot(s.n, s.l), 0.0)
    n_dot_v = rmax(dot(s.n, s.v), 1e-4)
    n_dot_h = rmax(dot(s.n, h), 0.0)
    h_dot_v = rmax(dot(h, s.v), 0.0)
    d = distribution_ggx(n_dot_h, s.roughness)
    g = geometry_smith_k(n_dot_v, n_dot_l, k)
    f = fresnel_schlick(h_dot_v, s.f0)
    specular = f * (d * g) / rmax(4.0 * n_dot_v * n_dot_l, 1e-6)
    diffuse = (1.0 - f) * (1.0 - s.metallic) * lambert_diffuse(s.albedo)
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
