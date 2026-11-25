"""The f64 model of punctual_light.slang and path_integrator.slang over path_integrator_test.slang's FloorScene."""
from dataclasses import dataclass, replace

import shader_model as sm
from shader_model import Real, Surface, Vec, below, clamp, dot, normalize, pow_, rmax, visit
from shader_model_lighting import length, surface_f0

TRACE_TMIN = 0.001
TRACE_TMAX = 10000.0
TRACE_SURFACE_OFFSET = 0.0001
TRACE_LIGHT_SPAN = 1.0 - 1e-4
PUNCTUAL_AT_INFINITY = 3.0e38
ZERO = (0.0, 0.0, 0.0)

# ---- common/punctual_light.slang ----


@dataclass
class PunctualLight:
    position: Vec
    kind: Real
    radiance: Vec
    range: Real
    direction: Vec
    cos_inner: Real
    cos_outer: Real


def unpack_punctual_light(a: Vec, b: Vec, c: Vec, d: Vec) -> PunctualLight:
    return PunctualLight(a.xyz, a.w, b.xyz, b.w, c.xyz, c.w, d.x)


def smoothstep(edge0: Real, edge1: Real, x: Real) -> Real:
    t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def punctual_incidence(light: PunctualLight, position: Vec) -> tuple[Vec, Real, Real]:
    """The unit direction to the light, its distance and the factor on its radiance."""
    if below(2.5, light.kind, "a directional light's kind"):
        visit("directional light")
        return normalize(-light.direction), Real(PUNCTUAL_AT_INFINITY), Real(1.0)
    to_light = light.position - position
    distance = rmax(length(to_light), 1e-4)
    l = to_light / distance
    attenuation = 1.0 / (distance * distance)
    # An input range of exactly 0 means unbounded, which no rounding moves.
    if light.range.v > 0.0:
        visit("ranged light")
        k = clamp(1.0 - pow_(distance / light.range, 4.0), 0.0, 1.0)
        attenuation = attenuation * (k * k)
    if below(1.5, light.kind, "a spot light's kind"):
        visit("spot light")
        attenuation = attenuation * smoothstep(light.cos_outer, light.cos_inner, dot(normalize(light.direction), -l))
    return l, distance, attenuation


# ---- common/path_integrator.slang ----


@dataclass
class TracedSurface:
    position: Vec
    face_normal: Vec
    normal: Vec
    albedo: Vec
    emissive: Vec
    metallic: Real
    roughness: Real
    unlit: bool


@dataclass
class LightSample:
    radiance: Vec
    l: Vec
    distance: Real


@dataclass
class Segment:
    origin: Vec
    tmin: Real
    direction: Vec
    tmax: Real


@dataclass
class FloorScene:
    """path_integrator_test.slang's ITracedScene: a floor at y = 0 under a sky, a wall past l.x 0.5."""

    floor: TracedSurface
    sun_l: Vec
    sun_rad: Vec
    light: PunctualLight
    lights: int
    furnace: Vec

    def closest(self, ray: Segment) -> tuple[bool, TracedSurface]:
        found = False
        if below(ray.direction.y, 0.0, "the ray against the floor's plane"):
            t = -ray.origin.y / ray.direction.y
            found = not below(t, ray.tmin, "the floor against tmin") and not below(ray.tmax, t, "the floor against tmax")
        visit("floor hit" if found else "floor missed")
        position = ray.origin + ray.direction * t if found else Vec(*ZERO)
        return found, replace(self.floor, position=position)

    def visible(self, l: Vec) -> bool:
        seen = below(l.x, 0.5, "the light against the wall")
        visit("unshadowed" if seen else "shadowed")
        return seen

    def furnace_on(self) -> bool:
        return below(0.5, self.furnace.w, "the furnace switch")


def brdf_at(s: TracedSurface, v: Vec, l: Vec, radiance: Vec) -> Vec:
    return sm.brdf_direct(Surface(s.normal, v, l, s.albedo, s.metallic, s.roughness, surface_f0(s.albedo, s.metallic)),
                          radiance)


def sun_sample(s: TracedSurface, v: Vec, l: Vec, radiance: Vec) -> LightSample:
    o = LightSample(Vec(*ZERO), l, Real(TRACE_TMAX))
    if not below(0.0, dot(s.normal, l), "the sun against the surface"):
        visit("sun below the surface")
        return o
    o.radiance = brdf_at(s, v, l, radiance)
    return o


def punctual_sample(s: TracedSurface, v: Vec, light: PunctualLight) -> LightSample:
    l, distance, attenuation = punctual_incidence(light, s.position)
    o = LightSample(Vec(*ZERO), l, distance * TRACE_LIGHT_SPAN)
    # A clamp or the cone's smoothstep pins a zero attenuation, and the light is continuous across either edge.
    if not attenuation.v > 0.0 or not below(0.0, dot(s.normal, l), "the light against the surface"):
        visit("light unlit")
        return o
    o.radiance = brdf_at(s, v, l, light.radiance * attenuation)
    return o


def shadowed(scene: FloorScene, light: LightSample) -> Vec:
    if not any(c.v > 0.0 for c in light.radiance.c):
        visit("no radiance, no shadow ray")
        return Vec(*ZERO)
    return light.radiance if scene.visible(light.l) else Vec(*ZERO)


def picked_punctual_sample(scene: FloorScene, s: TracedSurface, v: Vec, rng: int) -> tuple[LightSample, int]:
    if scene.lights == 0:
        visit("no punctual light")
        return LightSample(Vec(*ZERO), Vec(0.0, 1.0, 0.0), Real(0.0)), rng
    _, rng = sm.rng_next(rng)
    o = punctual_sample(s, v, scene.light)
    o.radiance = o.radiance * float(scene.lights)
    return o, rng


def next_event(scene: FloorScene, s: TracedSurface, v: Vec, rng: int) -> tuple[Vec, int]:
    if scene.furnace_on():
        visit("the furnace has no lights")
        return Vec(*ZERO), rng
    sun = sun_sample(s, v, scene.sun_l, scene.sun_rad)
    picked, rng = picked_punctual_sample(scene, s, v, rng)
    return Vec(*ZERO) + shadowed(scene, sun) + shadowed(scene, picked), rng


def furnace_surface(scene: FloorScene, s: TracedSurface) -> TracedSurface:
    return replace(s, albedo=Vec(1.0, 1.0, 1.0), emissive=Vec(*ZERO), unlit=False, metallic=scene.furnace.y,
                   roughness=clamp(scene.furnace.z, sm.MIN_ROUGHNESS, 1.0))


def path_miss(scene: FloorScene, d: Vec, bounce: int) -> Vec:
    if scene.furnace_on():
        visit("furnace sky")
        return Vec(scene.furnace.x, scene.furnace.x, scene.furnace.x)
    if bounce == 0:
        visit("camera sky")
        return d * 0.5 + 0.5
    visit("bounce sky")
    return (d * 0.5 + 0.5) * 0.25


def trace_path(scene: FloorScene, camera: Segment, max_bounces: int, rng: int,
               probe: bool) -> tuple[Vec, int, bool, Vec]:
    """The radiance, the rng after it and the depth probe's hit and position."""
    probing, ray = probe, camera
    depth_hit, depth_position = False, Vec(*ZERO)
    throughput, radiance, bounce = Vec(1.0, 1.0, 1.0), Vec(*ZERO), 0
    while probing or bounce < max_bounces:
        if not probing:
            alive, throughput, rng = sm.russian_roulette_survives(bounce, throughput, rng)
            if not alive:
                break
        hit, s = scene.closest(ray)
        if probing:
            visit("depth probed")
            depth_hit, depth_position, probing, ray = hit, s.position, False, camera
            continue
        if not hit:
            radiance = radiance + throughput * path_miss(scene, ray.direction, bounce)
            break
        if scene.furnace_on():
            s = furnace_surface(scene, s)
        if s.unlit:
            visit("unlit surface")
            radiance = radiance + throughput * s.albedo
            break
        v = -ray.direction
        if below(dot(s.normal, v), 1e-4, "the shading normal against the viewer"):
            visit("the face normal shades")
            s = replace(s, normal=s.face_normal)
        lit, rng = next_event(scene, s, v, rng)
        radiance = radiance + throughput * (s.emissive + lit)
        direction, weight, rng = sm.sample_bounce(
            Surface(s.normal, v, Vec(*ZERO), s.albedo, s.metallic, s.roughness, Vec(*ZERO)), s.face_normal, rng)
        if all(c.v == 0.0 for c in weight.c):
            break
        throughput = throughput * weight
        ray = Segment(s.position + s.face_normal * TRACE_SURFACE_OFFSET, Real(TRACE_TMIN), direction, Real(TRACE_TMAX))
        bounce += 1
    return radiance, rng, depth_hit, depth_position
