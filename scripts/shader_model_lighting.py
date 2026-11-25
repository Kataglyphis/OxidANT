"""The f64 model of the shared shadow and lighting modules, beside shader_model.py and on its rounding Real."""
import math
from collections.abc import Callable, Sequence

from shader_model import (
    Real,
    Vec,
    below,
    clamp,
    cross,
    dot,
    f32,
    fullscreen_ndc_to_uv,
    lerp,
    mul_mat_vec,
    normalize,
    rmax,
    rmin,
    saturate,
    sqrt,
    visit,
)

# ---- common/cascade_visibility.slang (and shadow_pcf.slang's receiver plane) ----


def select_cascade(view_depth: Real, splits: Vec, count: int) -> int:
    selected = 0
    if not below(view_depth, splits.x, "view depth against the first split"):
        visit("past the first split")
        selected = 1
    if not below(view_depth, splits.y, "view depth against the second split"):
        visit("past the second split")
        selected = 2
    if count - 1 < selected:
        visit("fewer cascades")
    return min(selected, count - 1)


def in_unit(x: Real, label: str) -> bool:
    return not below(x, 0.0, f"{label} against 0") and not below(1.0, x, f"{label} against 1")


def cascade_hit(rows: Sequence[Vec], world: Vec) -> tuple[Vec, Real, bool]:
    p = mul_mat_vec(rows, Vec(world.x, world.y, world.z, 1.0))
    proj = p.xyz / p.w
    uv = fullscreen_ndc_to_uv(proj.xy)
    inside = in_unit(uv.x, "uv.x") and in_unit(uv.y, "uv.y") and in_unit(proj.z, "depth")
    visit("inside" if inside else "outside")
    return uv, proj.z, inside


def cascade_bias(n_dot_l: Real, bias_scale: Real) -> Real:
    return rmax(0.002 * (1.0 - n_dot_l), 0.0005) * bias_scale


def receiver_plane_depth_slope(rows: Sequence[Vec], n: Vec, uv_y_sign: float) -> Vec:
    ax, ay, az = rows[0].xyz, rows[1].xyz, rows[2].xyz
    light_axis = cross(ax, ay)
    mx = dot(n, cross(ay, az))
    my = dot(n, cross(az, ax))
    mz = dot(n, light_axis)
    floor_z = 0.1 * sqrt(dot(light_axis, light_axis))
    if below(mz, 0.0, "receiver facing the light"):
        visit("receiver facing away")
        denom = rmin(mz, -floor_z)
    else:
        denom = rmax(mz, floor_z)
    return Vec(mx, my * uv_y_sign) * -2.0 / denom


def cascade_depth_slope(rows: Sequence[Vec], n: Vec) -> Vec:
    """WGSL's: fullscreen_ndc_to_uv here runs y down, so its sign is -1."""
    return receiver_plane_depth_slope(rows, n, -1.0)


def ramp_cascade_tap(uv: Vec, reference: Real) -> Real:
    return saturate(0.5 + 2.0 * (uv.x - 0.5) - 4.0 * (reference - 0.5))


def cascade_visibility(rows: Sequence[Vec], uv: Vec, depth: Real, n: Vec, n_dot_l: Real, bias_scale: Real,
                       texel: Vec, radius: int) -> Real:
    reference = depth - cascade_bias(n_dot_l, bias_scale)
    slope = cascade_depth_slope(rows, n)
    total = Real(0.0)
    for y in range(-radius, radius + 1):
        for x in range(-radius, radius + 1):
            offset = Vec(float(x), float(y)) * texel
            total = total + ramp_cascade_tap(uv + offset, reference + dot(slope, offset))
    return total / float((2 * radius + 1) * (2 * radius + 1))


# ---- common/punctual_shadow.slang ----


def unpack_punctual_shadow(d: Vec) -> tuple[int, Real, Real]:
    slot = int((d.y + 0.5).v) - 1
    visit("unshadowed" if slot < 0 else "shadowed")
    return slot, d.z, d.w


def punctual_shadow_depth(major: Real, near: Real, far: Real) -> Real:
    return far * (major - near) / ((far - near) * major)


def punctual_shadow_face(r: Vec) -> tuple[int, Vec]:
    a = Vec(*(Real(abs(c.v)) for c in r.c))
    if not below(a.x, a.y, "|x| against |y|") and not below(a.x, a.z, "|x| against |z|"):
        visit("x face")
        ma = a.x
        positive = below(0.0, r.x, "x sign")
        face = 0 if positive else 1
        ndc = Vec(-r.z if positive else r.z, -r.y)
    elif not below(a.y, a.z, "|y| against |z|"):
        visit("y face")
        ma = a.y
        positive = below(0.0, r.y, "y sign")
        face = 2 if positive else 3
        ndc = Vec(r.x, r.z if positive else -r.z)
    else:
        visit("z face")
        ma = a.z
        positive = below(0.0, r.z, "z sign")
        face = 4 if positive else 5
        ndc = Vec(r.x if positive else -r.x, -r.y)
    return face, ndc / rmax(ma, 1e-20)


def ramp_punctual_tap(d: Vec, reference: Real) -> Real:
    return saturate(0.5 + 0.1 * (d.x + 0.5 * d.y - 0.25 * d.z) - 4.0 * (reference - 0.9))


def length(v: Vec) -> Real:
    return sqrt(dot(v, v))


def punctual_shadow_visibility(near: Real, far: Real, light: Vec, world: Vec, n: Vec, l: Vec, width: Real) -> Real:
    texel = 2.0 * length(world - light) / width
    d = world + n * (1.5 * texel) + l * texel - light
    major = rmax(Real(abs(d.x.v)), rmax(Real(abs(d.y.v)), Real(abs(d.z.v))))
    if not below(major, far, "major axis against the far plane"):
        visit("beyond the far plane")
        return Real(1.0)
    reference = punctual_shadow_depth(rmax(major, near), near, far)
    axis = normalize(d)
    if below(Real(abs(axis.y.v)), 0.99, "axis against the pole"):
        up = Vec(0.0, 1.0, 0.0)
    else:
        visit("near the pole")
        up = Vec(1.0, 0.0, 0.0)
    tangent = normalize(cross(up, axis))
    bitangent = cross(axis, tangent)
    spread = 3.0 * major / width
    lit = ramp_punctual_tap(d, reference)
    lit = lit + ramp_punctual_tap(d + tangent * spread, reference)
    lit = lit + ramp_punctual_tap(d - tangent * spread, reference)
    lit = lit + ramp_punctual_tap(d + bitangent * spread, reference)
    lit = lit + ramp_punctual_tap(d - bitangent * spread, reference)
    return lit * 0.2


# ---- common/surface_lighting.slang ----

HDR_MAX = 65000.0


def surface_f0(albedo: Vec, metallic: Real) -> Vec:
    return lerp(Vec(0.04, 0.04, 0.04), albedo, metallic)


def surface_lighting(sun: Vec, visibility: Real, strength: Real, punctual: Vec, ambient: Vec, emissive: Vec) -> Vec:
    shadowed = Real(1.0) * (1.0 - strength) + visibility * strength
    total = sun * shadowed + punctual + ambient + emissive
    if any(c.v > f32(HDR_MAX) for c in total.c):
        visit("capped at HDR_MAX")
    return Vec(*(clamp(c, 0.0, HDR_MAX) for c in total.c))


# ---- common/gbuffer.slang ----


def gbuffer_unlit(normal_metallic: Vec) -> bool:
    unlit = below(normal_metallic.w, -0.5, "metallic against the unlit sign")
    visit("unlit" if unlit else "lit")
    return unlit


# ---- common/cloud_upsample.slang ----

CLOUD_UPSAMPLE_DEPTH_SHARPNESS = 16.0


def cloud_upsample_depth_weight(pixel_distance: Real, texel_distance: Real) -> Real:
    relative = Real(abs((pixel_distance - texel_distance).v)) / rmax(rmin(pixel_distance, texel_distance), 1e-4)
    return Real(math.exp((-rmin(relative, 100.0) * CLOUD_UPSAMPLE_DEPTH_SHARPNESS).v))


def half_texel_depth(depth: Callable[[int, int], Real], texel: tuple[int, int], size: tuple[int, int]) -> Real:
    nearest = Real(1.0)
    for i in range(4):
        x = min(texel[0] * 2 + (i & 1), size[0] - 1)
        y = min(texel[1] * 2 + (i >> 1), size[1] - 1)
        nearest = rmin(nearest, depth(x, y))
    return nearest


def cloud_upsample(depth: Callable[[int, int], Real], half: Callable[[int, int], Vec],
                   distance: Callable[[Real], Real], pixel: tuple[int, int], half_size: tuple[int, int],
                   depth_size: tuple[int, int]) -> Vec:
    pixel_distance = distance(depth(*pixel))
    position = Vec(float(pixel[0]), float(pixel[1])) + 0.5
    position = position * 0.5 - 0.5
    base = (math.floor(position.x.v), math.floor(position.y.v))
    f = Vec(position.x - float(base[0]), position.y - float(base[1]))
    weighted, bilinear, weight_sum = Vec(0.0, 0.0, 0.0, 0.0), Vec(0.0, 0.0, 0.0, 0.0), Real(0.0)
    for i in range(4):
        corner = (i & 1, i >> 1)
        texel = (min(max(base[0] + corner[0], 0), half_size[0] - 1), min(max(base[1] + corner[1], 0), half_size[1] - 1))
        axis = Vec(*((1.0 - f.c[k]) * (1.0 - float(corner[k])) + f.c[k] * float(corner[k]) for k in range(2)))
        w = axis.x * axis.y
        value = half(*texel)
        texel_distance = distance(half_texel_depth(depth, texel, depth_size))
        depth_weight = w * cloud_upsample_depth_weight(pixel_distance, texel_distance)
        weighted = weighted + value * depth_weight
        weight_sum = weight_sum + depth_weight
        bilinear = bilinear + value * w
    if below(1e-4, weight_sum, "the depth weights' sum against 1e-4"):
        visit("depth weighted")
        return weighted / weight_sum
    visit("lone sliver")
    return bilinear
