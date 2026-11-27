"""The shadow, lighting and sky modules' cases and evaluations for generate-shader-vectors.py: cascade_visibility,
punctual_shadow, surface_lighting, gbuffer, cloud_upsample and drawn_sky."""
import shader_model as sm
import shader_model_lighting as sl
from shader_vector_format import (
    CHAINED,
    SMOOTH,
    Draw,
    Module,
    Values,
    flat,
    merged,
    one,
    out,
    ports,
    unit,
    word,
)


def evaluate_cascade_visibility(i: Values) -> dict[str, list[float]]:
    rows = [sm.Vec(*i[f"row{k}"]) for k in range(4)]
    n, n_dot_l, bias_scale = sm.vec3(i["n"]), one(i, "n_dot_l"), one(i, "bias_scale")
    uv, depth, inside = sl.cascade_hit(rows, sm.vec3(i["world"]))
    slope = sl.cascade_depth_slope(rows, n)
    return {
        "select_cascade": [sl.select_cascade(one(i, "view_depth"), sm.Vec(*i["splits"]), word(i, "count"))],
        "cascade_hit.ndc": flat(sm.fullscreen_uv_to_ndc(uv)),
        "cascade_hit.depth": flat(depth),
        "cascade_hit.inside": [int(inside)],
        "cascade_bias": flat(sl.cascade_bias(n_dot_l, bias_scale)),
        "cascade_depth_slope.y_up": flat(slope.x, -slope.y),
        "cascade_visibility": flat(sl.cascade_visibility(
            rows, uv, depth, n, n_dot_l, bias_scale, sm.Vec(*i["texel"]), word(i, "radius"))),
    }


def evaluate_punctual_shadow(i: Values) -> dict[str, list[float]]:
    slot, near, far = sl.unpack_punctual_shadow(sm.Vec(*i["row"]))
    face, ndc = sl.punctual_shadow_face(sm.vec3(i["r"]))
    uv = sm.fullscreen_ndc_to_uv(ndc)
    return {
        "unpack_punctual_shadow.slot_plus_one": [slot + 1],
        "unpack_punctual_shadow.range": flat(near, far),
        "punctual_shadow_depth": flat(sl.punctual_shadow_depth(one(i, "major"), near, far)),
        "punctual_shadow_face.face": [face],
        "punctual_shadow_face.ndc": flat(ndc),
        "punctual_shadow_face_coord.ndc": flat(sm.fullscreen_uv_to_ndc(uv)),
        "punctual_shadow_face_coord.layer": [float(max(slot, 0) * 6 + face)],
        "punctual_shadow_visibility": flat(sl.punctual_shadow_visibility(
            near, far, sm.vec3(i["light"]), sm.vec3(i["world"]), sm.vec3(i["n"]), sm.vec3(i["l"]), one(i, "width"))),
    }


def evaluate_surface_lighting(i: Values) -> dict[str, list[float]]:
    return {
        "surface_f0": flat(sl.surface_f0(sm.vec3(i["albedo"]), one(i, "metallic"))),
        "surface_lighting": flat(sl.surface_lighting(
            sm.vec3(i["sun"]), one(i, "visibility"), one(i, "strength"), sm.vec3(i["punctual"]),
            sm.vec3(i["ambient"]), sm.vec3(i["emissive"]))),
        "screen_space_occlusion": flat(sl.screen_space_occlusion(
            one(i, "occlusion"), one(i, "occlusion_strength"), word(i, "blended") != 0)),
    }


def evaluate_gbuffer(i: Values) -> dict[str, list[float]]:
    n, albedo, emissive, stored = sm.vec3(i["n"]), sm.vec3(i["albedo"]), sm.vec3(i["emissive"]), sm.Vec(*i["stored"])
    metallic = -1.0 if word(i, "unlit") != 0 else i["metallic"][0]
    return {
        "gbuffer_pack.normal_metallic": flat(n, sm.real(metallic)),
        "gbuffer_pack.albedo_occlusion": flat(albedo, one(i, "occlusion")),
        "gbuffer_pack.roughness_emissive": flat(one(i, "roughness"), emissive),
        "gbuffer_unlit": [int(sl.gbuffer_unlit(stored))],
        "gbuffer_unpack.position": flat(sm.vec3(i["position"])),
        "gbuffer_unpack.n": flat(sm.normalize_or(stored.xyz, sm.Vec(0.0, 1.0, 0.0))),
        "gbuffer_unpack.albedo": flat(albedo),
        "gbuffer_unpack.material": flat(stored.w, one(i, "roughness"), one(i, "occlusion")),
        "gbuffer_unpack.emissive": flat(emissive),
    }


def evaluate_cloud_upsample(i: Values) -> dict[str, list[float]]:
    spot = (int(i["spot"][0]), int(i["spot"][1]))
    spot_depth, rest_depth, scale = one(i, "spot_depth"), one(i, "rest_depth"), one(i, "scale")

    def depth(x: int, y: int) -> sm.Real:
        return spot_depth if (x, y) == spot else rest_depth

    def half(x: int, y: int) -> sm.Vec:
        return sm.Vec(sm.Real(x) * 0.1, sm.Real(y) * 0.1, sm.Real(x + y) * 0.05, 1.0)

    def pair(name: str) -> tuple[int, int]:
        return int(i[name][0]), int(i[name][1])

    return {
        "half_texel_depth": flat(sl.half_texel_depth(depth, pair("texel"), pair("depth_size"))),
        "cloud_upsample": flat(sl.cloud_upsample(
            depth, half, lambda d: d * scale, pair("pixel"), pair("half_size"), pair("depth_size"))),
    }


def cascade_visibility_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "row0": [0.1, 0.0, 0.0, 0.0], "row1": [0.0, 0.1, 0.0, 0.0], "row2": [0.0, 0.0, 0.05, 0.4],
        "row3": [0.0, 0.0, 0.0, 1.0], "world": [1.0, 2.0, 3.0], "view_depth": [5.0], "splits": [10.0, 30.0],
        "count": [3], "n": unit(0.2, 1.0, 0.3), "n_dot_l": [0.6], "bias_scale": [1.0],
        "texel": [1.0 / 2048.0, 1.0 / 2048.0], "radius": [1],
    }
    rows: list[tuple[str, Values]] = [
        ("an orthographic box holds the point; three cascades, the first selected", {}),
        ("past the first split", {"view_depth": [15.0]}),
        ("past the second split", {"view_depth": [40.0]}),
        ("past the second split with two cascades selects the last", {"view_depth": [40.0], "count": [2]}),
        ("one cascade is always the selected one", {"view_depth": [40.0], "count": [1]}),
        ("a point right of the box is outside", {"world": [20.0, 2.0, 3.0]}),
        ("a point below the box is outside", {"world": [1.0, -12.0, 3.0]}),
        ("a point past the far plane is outside", {"world": [1.0, 2.0, 20.0]}),
        ("a receiver facing away from the light", {"n": unit(0.1, 0.2, -1.0)}),
        ("a grazing receiver takes the slope's floor", {"n": unit(1.0, 0.0, 0.01)}),
        ("radius 0 is a single tap", {"radius": [0]}),
        ("radius 2 on a map twice as tall as wide", {"radius": [2], "texel": [1.0 / 512.0, 1.0 / 1024.0]}),
        ("light along the normal keeps the bias floor, scaled", {"n_dot_l": [1.0], "bias_scale": [2.5]}),
        ("light at the horizon takes the full slope bias", {"n_dot_l": [0.0], "bias_scale": [0.5]}),
        ("a perspective light divides by w", {"row3": [0.0, 0.0, 0.1, 1.0], "world": [1.5, -2.0, 3.0]}),
    ]
    draw = Draw(11)
    for index in range(4):
        rows.append((f"seeded receiver {index}", {
            "world": draw.nums(-3.0, 3.0), "n": draw.direction([0.0, 0.0, 1.0]), "n_dot_l": [draw.num(0.0, 1.0)],
            "bias_scale": [draw.num(0.5, 2.0)], "view_depth": [draw.num(0.0, 8.0)], "radius": [draw.whole(0, 3)]}))
    return merged(base, rows)


def punctual_shadow_cases() -> list[tuple[str, Values]]:
    light, world = [0.0, 3.0, 0.0], [2.0, 0.5, 1.0]
    base: Values = {
        "row": [0.0, 3.0, 0.1, 25.0], "major": [5.0], "r": unit(0.8, 0.3, -0.2), "light": light, "world": world,
        "n": [0.0, 1.0, 0.0], "l": unit(*(a - b for a, b in zip(light, world, strict=True))), "width": [512.0],
    }
    rows: list[tuple[str, Values]] = [
        ("slot 2, a +X face direction", {}),
        ("a -X face direction", {"r": unit(-0.9, 0.2, 0.1)}),
        ("a -Y face direction", {"r": unit(0.2, -0.9, 0.3)}),
        ("a +Y face direction", {"r": unit(-0.3, 0.9, -0.2)}),
        ("a +Z face direction", {"r": unit(-0.1, 0.2, 0.95)}),
        ("a -Z face direction", {"r": unit(0.25, -0.15, -0.9)}),
        ("an unshadowed light: slot -1", {"row": [0.0, 0.0, 0.1, 25.0]}),
        ("the depth at the near plane is 0", {"major": [0.1]}),
        ("the depth at the far plane is 1", {"major": [25.0]}),
        ("a receiver past the far plane is lit", {"row": [0.0, 1.0, 0.1, 1.5]}),
        ("a light straight below takes the other tangent", {
            "light": [2.0, -4.0, 1.0], "n": [0.0, -1.0, 0.0], "l": [0.0, -1.0, 0.0]}),
        ("a receiver nearer than the near plane compares at it", {"row": [0.0, 1.0, 3.0, 25.0]}),
        ("a coarse 64-texel face spreads its taps wider", {"width": [64.0]}),
    ]
    draw = Draw(12)
    for index in range(4):
        w, lp = draw.nums(-2.0, 2.0), draw.nums(-2.0, 2.0)
        rows.append((f"seeded light {index}", {
            "r": draw.direction(), "light": lp, "world": w, "n": draw.direction(),
            "l": unit(*(a - b for a, b in zip(lp, w, strict=True))), "major": [draw.num(0.2, 20.0)]}))
    return merged(base, rows)


def surface_lighting_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "albedo": [0.8, 0.2, 0.1], "metallic": [0.0], "sun": [1.5, 1.2, 0.9], "visibility": [0.25],
        "strength": [1.0], "punctual": [0.1, 0.2, 0.05], "ambient": [0.05, 0.06, 0.08], "emissive": [0.0, 0.0, 0.0],
        "occlusion": [0.4], "occlusion_strength": [0.7], "blended": [0],
    }
    rows: list[tuple[str, Values]] = [
        ("a dielectric in a quarter of the sun's light", {}),
        ("a metal's f0 is its albedo", {"metallic": [1.0]}),
        ("a half metal", {"metallic": [0.5]}),
        ("strength 0 leaves the sun unshadowed", {"strength": [0.0]}),
        ("strength 0.6 lifts a full shadow to 0.4", {"visibility": [0.0], "strength": [0.6]}),
        ("full visibility", {"visibility": [1.0]}),
        ("the shadow leaves punctual, ambient and emissive alone", {
            "visibility": [0.0], "emissive": [2.0, 0.5, 0.0]}),
        ("an emissive strength above 1", {"emissive": [40.0, 12.0, 3.0]}),
        ("a radiance past the half float's range caps at HDR_MAX", {"sun": [70000.0, 1.0, 1.0], "visibility": [1.0]}),
        ("negative terms floor at zero", {"sun": [0.0, 0.0, 0.0], "punctual": [-0.5, 0.0, 0.0], "ambient": [0.0, 0.0, 0.0]}),
        ("SSAO strength 0 leaves the ambient whole", {"occlusion": [0.1], "occlusion_strength": [0.0]}),
        ("SSAO strength 1 takes the occlusion as it is", {"occlusion": [0.25], "occlusion_strength": [1.0]}),
        ("a BLEND surface takes no SSAO: its texel is the surface behind", {"blended": [1]}),
        ("an occlusion and strength past their range saturate", {"occlusion": [1.5], "occlusion_strength": [2.0]}),
    ]
    draw = Draw(13)
    for index in range(4):
        rows.append((f"seeded terms {index}", {
            "albedo": draw.nums(0.0, 1.0), "metallic": [draw.num(0.0, 1.0)], "sun": draw.nums(0.0, 5.0),
            "visibility": [draw.num(0.0, 1.0)], "strength": [draw.num(0.0, 1.0)], "punctual": draw.nums(0.0, 2.0),
            "ambient": draw.nums(0.0, 1.0), "emissive": draw.nums(0.0, 3.0), "occlusion": [draw.num(0.0, 1.0)],
            "occlusion_strength": [draw.num(0.0, 1.0)], "blended": [index % 2]}))
    return merged(base, rows)


def gbuffer_cases() -> list[tuple[str, Values]]:
    n = unit(0.2, 0.9, -0.3)
    base: Values = {
        "position": [1.0, -2.0, 3.5], "n": n, "albedo": [0.8, 0.5, 0.2], "metallic": [0.3], "roughness": [0.6],
        "occlusion": [0.9], "emissive": [0.0, 0.0, 0.0], "unlit": [0], "stored": n + [0.3],
    }
    rows: list[tuple[str, Values]] = [
        ("a lit dielectric round trips", {}),
        ("an unlit surface stores metallic -1", {"unlit": [1], "stored": n + [-1.0]}),
        ("a metal at full occlusion", {"metallic": [1.0], "occlusion": [1.0], "stored": n + [1.0]}),
        ("an emissive strength above 1 survives the half float", {"emissive": [12.0, 3.5, 0.25]}),
        ("a zero normal reads back facing up", {"stored": [0.0, 0.0, 0.0, 0.3]}),
        ("a blended, shortened normal reads back unit length", {"stored": [0.1, 0.45, -0.15, 0.3]}),
        ("metallic 0 is lit", {"metallic": [0.0], "stored": n + [0.0]}),
        ("a sign just below -0.5 is unlit", {"stored": n + [-0.6]}),
        ("a sign just above -0.5 is lit", {"stored": n + [-0.4]}),
        ("roughness at its floor and full occlusion", {"roughness": [0.045], "occlusion": [0.0]}),
    ]
    draw = Draw(14)
    for index in range(4):
        d = draw.direction()
        rows.append((f"seeded surface {index}", {
            "position": draw.nums(-10.0, 10.0), "n": d, "albedo": draw.nums(0.0, 1.0),
            "metallic": [draw.num(0.0, 1.0)], "roughness": [draw.num(0.05, 1.0)], "occlusion": [draw.num(0.0, 1.0)],
            "emissive": draw.nums(0.0, 4.0), "unlit": [index % 2], "stored": d + [draw.num(-1.0, 1.0)]}))
    return merged(base, rows)


def cloud_upsample_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "pixel": [5, 7], "texel": [2, 3], "half_size": [8, 6], "depth_size": [16, 12], "spot": [20, 20],
        "spot_depth": [0.2], "rest_depth": [0.6], "scale": [10.0],
    }
    rows: list[tuple[str, Values]] = [
        ("one depth everywhere: the bilinear blend", {}),
        ("an even pixel takes its texel and the ones before it", {"pixel": [6, 4]}),
        ("the top-left pixel clamps to the first texel", {"pixel": [0, 0], "texel": [0, 0]}),
        ("the bottom-right pixel clamps to the last texel", {"pixel": [15, 11], "texel": [7, 5]}),
        ("a near spot inside a texel sets its depth", {"spot": [4, 6], "texel": [2, 3]}),
        ("a near spot pulls its texel off a far pixel's blend", {"spot": [4, 6], "pixel": [5, 7]}),
        ("the near spot itself keeps its own texel", {"spot": [5, 7], "pixel": [5, 7]}),
        ("a lone far pixel among near ones: the bilinear blend", {
            "spot": [5, 7], "pixel": [5, 7], "spot_depth": [0.8], "rest_depth": [0.2]}),
        ("an odd-sized depth target clamps its last half texel's pixels", {
            "depth_size": [15, 11], "pixel": [14, 10], "texel": [7, 5]}),
        ("a far spot in a texel leaves its nearest depth", {"spot": [4, 6], "spot_depth": [0.9], "texel": [2, 3]}),
        ("a long scale keeps the relative weight", {"spot": [4, 6], "scale": [1000.0]}),
        ("a spot one texel over", {"spot": [7, 7], "pixel": [5, 7]}),
    ]
    return merged(base, rows)


def evaluate_drawn_sky(i: Values) -> dict[str, list[float]]:
    return {"drawn_sky": flat(sl.drawn_sky(
        one(i, "mode"), sm.vec3(i["cube"]), sm.vec3(i["dir"]), sm.vec3(i["light_dir"]), one(i, "sun_intensity")))}


def drawn_sky_cases() -> list[tuple[str, Values]]:
    base: Values = {"mode": [0.0], "cube": [0.3, 0.5, 0.8], "dir": unit(0.3, 0.8, 0.5), "light_dir": [0.5, 0.8, 0.3],
                    "sun_intensity": [5.0]}
    rows: list[tuple[str, Values]] = [
        ("procedural: the gradient above the horizon and the sun's glow", {}),
        ("procedural: below the horizon the ground side", {"dir": unit(0.6, -0.2, 0.7)}),
        ("procedural: far below, the ground colour itself", {"dir": unit(0.1, -0.9, 0.2)}),
        ("procedural: straight up, the zenith", {"dir": [0.0, 1.0, 0.0]}),
        ("procedural: near the sun the 1200th power's disk", {"dir": unit(0.5, 0.8, 0.31)}),
        ("procedural: a sun behind the view adds nothing", {"light_dir": [-0.3, -0.8, -0.5]}),
        ("procedural: a dark sun adds nothing", {"dir": unit(0.5, 0.8, 0.31), "sun_intensity": [0.0]}),
        ("procedural: a light direction of any length", {"light_dir": [5.0, 8.0, 3.0]}),
        ("the cube map's display colour untonemapped", {"mode": [1.0]}),
        ("the cube map ignores the direction and the sun", {"mode": [1.0], "dir": unit(0.6, -0.2, 0.7),
                                                             "sun_intensity": [0.0]}),
        ("a cube texel outside [0, 1] clamps first", {"mode": [1.0], "cube": [-0.2, 1.4, 0.95]}),
        ("a black cube texel", {"mode": [1.0], "cube": [0.0, 0.0, 0.0]}),
        ("switched off: the navy untonemapped", {"mode": [2.0]}),
        ("switched off ignores the cube map", {"mode": [2.0], "cube": [1.0, 1.0, 1.0]}),
    ]
    draw = Draw(14)
    for index in range(4):
        rows.append((f"seeded procedural sky {index}", {
            "dir": unit(*draw.nums(-1.0, 1.0)), "light_dir": draw.nums(-1.0, 1.0), "sun_intensity": [draw.num(0.0, 10.0)]}))
    return merged(base, rows)


MODULES = [
    Module("cascade_visibility", ports(
        "row0:4 row1:4 row2:4 row3:4 world:3 view_depth splits:2 count/i32 n:3 n_dot_l bias_scale texel:2 radius/i32"), [
        out("select_cascade", 1), out("cascade_hit.ndc", 2, CHAINED), out("cascade_hit.depth", 1, CHAINED),
        out("cascade_hit.inside", 1), out("cascade_bias", 1, SMOOTH), out("cascade_depth_slope.y_up", 2, CHAINED),
        out("cascade_visibility", 1, CHAINED),
    ], evaluate_cascade_visibility, cascade_visibility_cases(), {
        "past the first split", "past the second split", "fewer cascades", "inside", "outside",
        "receiver facing away"}),
    Module("punctual_shadow", ports("row:4 major r:3 light:3 world:3 n:3 l:3 width"), [
        out("unpack_punctual_shadow.slot_plus_one", 1), out("unpack_punctual_shadow.range", 2, SMOOTH),
        out("punctual_shadow_depth", 1, CHAINED), out("punctual_shadow_face.face", 1),
        out("punctual_shadow_face.ndc", 2, CHAINED), out("punctual_shadow_face_coord.ndc", 2, CHAINED),
        out("punctual_shadow_face_coord.layer", 1, SMOOTH), out("punctual_shadow_visibility", 1, CHAINED),
    ], evaluate_punctual_shadow, punctual_shadow_cases(), {
        "shadowed", "unshadowed", "x face", "y face", "z face", "beyond the far plane", "near the pole"}),
    Module("surface_lighting", ports(
        "albedo:3 metallic sun:3 visibility strength punctual:3 ambient:3 emissive:3 occlusion occlusion_strength "
        "blended/u32"), [
        out("surface_f0", 3, SMOOTH), out("surface_lighting", 3, CHAINED), out("screen_space_occlusion", 1, SMOOTH),
    ], evaluate_surface_lighting, surface_lighting_cases(), {"capped at HDR_MAX", "blended", "saturated"}),
    Module("gbuffer", ports(
        "position:3 n:3 albedo:3 metallic roughness occlusion emissive:3 unlit/u32 stored:4"), [
        out("gbuffer_pack.normal_metallic", 4, SMOOTH), out("gbuffer_pack.albedo_occlusion", 4, SMOOTH),
        out("gbuffer_pack.roughness_emissive", 4, SMOOTH), out("gbuffer_unlit", 1),
        out("gbuffer_unpack.position", 3, SMOOTH), out("gbuffer_unpack.n", 3, SMOOTH),
        out("gbuffer_unpack.albedo", 3, SMOOTH), out("gbuffer_unpack.material", 3, SMOOTH),
        out("gbuffer_unpack.emissive", 3, SMOOTH),
    ], evaluate_gbuffer, gbuffer_cases(), {"unlit", "lit", "zero vector"}),
    Module("cloud_upsample", ports(
        "pixel:2/i32 texel:2/i32 half_size:2/i32 depth_size:2/i32 spot:2/i32 spot_depth rest_depth scale"), [
        out("half_texel_depth", 1, SMOOTH), out("cloud_upsample", 4, CHAINED),
    ], evaluate_cloud_upsample, cloud_upsample_cases(), {"depth weighted", "lone sliver"}),
    # pow(cos, 1200) multiplies the dot product's rounding by 1200, so the sun's disk needs a looser bound.
    Module("drawn_sky", ports("mode cube:3 dir:3 light_dir:3 sun_intensity"), [out("drawn_sky", 3, (1e-5, 2e-3))],
           evaluate_drawn_sky, drawn_sky_cases(), {
               "procedural", "cube map", "switched off", "sky side", "ground side", "display clamp"}),
]
