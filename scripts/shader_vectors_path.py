"""path_integrator's cases and evaluation for generate-shader-vectors.py, over path_integrator_test.slang's FloorScene."""
from dataclasses import replace

import shader_model as sm
import shader_model_path as sp
from shader_vector_format import CHAINED, Draw, Module, Values, flat, merged, one, out, ports, unit, word


def scene_of(i: Values) -> sp.FloorScene:
    floor = sp.TracedSurface(sm.Vec(0.0, 0.0, 0.0), sm.Vec(0.0, 1.0, 0.0), sm.vec3(i["normal"]), sm.vec3(i["albedo"]),
                             sm.vec3(i["emissive"]), one(i, "metallic"), one(i, "roughness"), word(i, "unlit") != 0)
    light = sp.unpack_punctual_light(*(sm.Vec(*i[f"light_{row}"]) for row in "abcd"))
    return sp.FloorScene(floor, sm.vec3(i["sun_l"]), sm.vec3(i["sun_rad"]), light, word(i, "lights"),
                         sm.Vec(*i["furnace"]))


def evaluate_path_integrator(i: Values) -> dict[str, list[float]]:
    scene = scene_of(i)
    s = replace(scene.floor, position=sm.vec3(i["position"]))
    v = sm.vec3(i["v"])
    camera = sp.Segment(sm.vec3(i["origin"]), sm.Real(0.0), sm.vec3(i["direction"]), one(i, "tmax"))
    rng = word(i, "rng")
    sun = sp.sun_sample(s, v, scene.sun_l, scene.sun_rad)
    punctual = sp.punctual_sample(s, v, scene.light)
    picked, picked_rng = sp.picked_punctual_sample(scene, s, v, rng)
    lit, event_rng = sp.next_event(scene, s, v, rng)
    white = sp.furnace_surface(scene, s)
    radiance, path_rng, depth_hit, depth_position = sp.trace_path(
        scene, camera, word(i, "max_bounces"), rng, word(i, "probe") != 0)
    return {
        "no_surface": [0.0, 0.0, 0.0, 0.0],
        "unlit_sample": [0.0, 0.0, 0.0, 0.0],
        "unlit_sample.l": [0.0, 1.0, 0.0],
        "sun_sample.radiance": flat(sun.radiance),
        "sun_sample.l": flat(sun.l),
        "sun_sample.distance": flat(sun.distance),
        "punctual_sample.radiance": flat(punctual.radiance),
        "punctual_sample.l": flat(punctual.l),
        "punctual_sample.distance": flat(punctual.distance),
        "shadowed": flat(sp.shadowed(scene, sun)),
        "shadowed.punctual": flat(sp.shadowed(scene, punctual)),
        "picked_punctual_sample.radiance": flat(picked.radiance),
        "picked_punctual_sample.rng": [picked_rng],
        "next_event": flat(lit),
        "next_event.rng": [event_rng],
        "furnace_surface": flat(white.albedo.x, white.emissive.x, white.metallic, white.roughness),
        "furnace_surface.unlit": [float(white.unlit)],
        "path_miss": flat(sp.path_miss(scene, camera.direction, 0)),
        "path_miss.bounce": flat(sp.path_miss(scene, camera.direction, 1)),
        "no_depth": [0],
        "trace_path": flat(radiance),
        "trace_path.rng": [path_rng],
        "trace_path.depth_hit": [int(depth_hit)],
        "trace_path.depth_position": flat(depth_position),
    }


def path_integrator_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "normal": [0.0, 1.0, 0.0], "albedo": [0.8, 0.6, 0.4], "emissive": [0.1, 0.0, 0.0], "metallic": [0.2], "roughness": [0.5], "unlit": [0],
        "sun_l": unit(0.3, 0.8, 0.5), "sun_rad": [5.0, 4.85, 4.6],
        "light_a": [1.0, 2.0, 0.5, 1.0], "light_b": [10.0, 8.0, 6.0, 0.0], "light_c": [0.0, -1.0, 0.0, 0.95],
        "light_d": [0.85, 0.0, 0.0, 0.0], "lights": [1], "furnace": [0.0, 0.0, 0.5, 0.0],
        "origin": [0.0, 1.5, 3.0], "direction": unit(0.1, -0.5, -0.8), "tmax": [100.0], "max_bounces": [4],
        "rng": [12345], "probe": [0], "position": [0.2, 0.0, -0.4], "v": unit(0.1, 0.7, 0.6),
    }
    rows: list[tuple[str, Values]] = [
        ("a camera ray hits the floor, the sun and a point light light it, the bounce sees the sky", {}),
        ("a camera ray that misses sees the camera sky", {"direction": unit(0.2, 0.6, -0.7)}),
        ("the depth probe takes the first pass and finds the floor", {"probe": [1]}),
        ("the depth probe of a ray that misses", {"probe": [1], "direction": unit(0.2, 0.6, -0.7)}),
        ("the floor past tmax is a miss", {"tmax": [1.0]}),
        ("the white furnace: a uniform sky, no lights, a white dielectric", {"furnace": [1.0, 0.0, 0.5, 1.0]}),
        ("the white metal furnace clamps its roughness to the floor", {"furnace": [0.8, 1.0, 0.01, 1.0]}),
        ("an unlit floor ends the path at its albedo", {"unlit": [1]}),
        ("no punctual light draws no number", {"lights": [0]}),
        ("one of three punctual lights, times three", {"lights": [3]}),
        ("a spot light inside its cone", {"light_a": [0.3, 2.0, -0.3, 2.0]}),
        ("a spot light at its cone's edge", {"light_a": [1.0, 2.0, 0.5, 2.0]}),
        ("a directional punctual light", {"light_a": [0.0, 0.0, 0.0, 3.0], "light_c": [-0.3, -0.8, -0.5, 0.95]}),
        ("a point light inside its range", {"light_b": [10.0, 8.0, 6.0, 4.0]}),
        ("a point light out of its range", {"light_b": [10.0, 8.0, 6.0, 1.5]}),
        ("the wall blocks a light towards +x", {"light_a": [5.0, 1.0, 0.0, 1.0]}),
        ("a light below the floor is no light", {"light_a": [1.0, -2.0, 0.5, 1.0]}),
        ("the sun below the floor", {"sun_l": unit(0.3, -0.8, 0.5)}),
        ("the wall blocks the sun", {"sun_l": unit(0.8, 0.5, 0.2)}),
        ("one segment: the hit's own light and no bounce", {"max_bounces": [1]}),
        ("a smooth metal floor", {"metallic": [1.0], "roughness": [0.1]}),
        ("a shading normal tilted towards the view lights by itself", {"normal": unit(0.2, 0.9, 0.3)}),
        ("a shading normal tilted away from the view: the face normal shades", {"normal": unit(0.0, 0.1, -1.0)}),
    ]
    draw = Draw(15)
    for index in range(3):
        rows.append((f"seeded stream {index}", {"rng": [int(draw.num(1.0, 4.0e9))], "v": unit(*draw.nums(0.2, 1.0))}))
    return merged(base, rows)


MODULES = [
    Module("path_integrator", ports(
        "normal:3 albedo:3 emissive:3 metallic roughness unlit/u32 sun_l:3 sun_rad:3 light_a:4 light_b:4 light_c:4 light_d:4 "
        "lights/u32 furnace:4 origin:3 direction:3 tmax max_bounces/u32 rng/u32 probe/u32 position:3 v:3"), [
        out("no_surface", 4, CHAINED), out("unlit_sample", 4, CHAINED), out("unlit_sample.l", 3, CHAINED),
        out("sun_sample.radiance", 3, CHAINED), out("sun_sample.l", 3, CHAINED), out("sun_sample.distance", 1, CHAINED),
        out("punctual_sample.radiance", 3, CHAINED), out("punctual_sample.l", 3, CHAINED),
        out("punctual_sample.distance", 1, CHAINED), out("shadowed", 3, CHAINED), out("shadowed.punctual", 3, CHAINED),
        out("picked_punctual_sample.radiance", 3, CHAINED), out("picked_punctual_sample.rng", 1),
        out("next_event", 3, CHAINED), out("next_event.rng", 1), out("furnace_surface", 4, CHAINED),
        out("furnace_surface.unlit", 1, CHAINED), out("path_miss", 3, CHAINED), out("path_miss.bounce", 3, CHAINED),
        out("no_depth", 1), out("trace_path", 3, CHAINED), out("trace_path.rng", 1), out("trace_path.depth_hit", 1),
        out("trace_path.depth_position", 3, CHAINED),
    ], evaluate_path_integrator, path_integrator_cases(), {
        "floor hit", "floor missed", "shadowed", "unshadowed", "no radiance, no shadow ray", "sun below the surface",
        "light unlit", "no punctual light", "directional light", "ranged light", "spot light",
        "the furnace has no lights", "furnace sky", "camera sky", "bounce sky", "depth probed", "unlit surface",
        "the face normal shades"}),
]
