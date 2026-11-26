#!/usr/bin/env python3
"""Write shaders/tests/vectors/<module>.json: each shared Slang module's cases with f64 references.

The references come from shader_model.py, never from the kernel. Each case runs twice, in f64 and with every
operation rounded to f32; a case whose f32 run strays past an eighth of a tolerance, or whose branch sits closer to
its edge than a device's rounding could move it, stops the generator: pick another input. `--check` compares the
committed files with a fresh run. docs/shaders.md#vector-kernels describes the format and how to add a module.
"""
import argparse
import json
import math
import struct
import sys
from pathlib import Path

sys.dont_write_bytecode = True
import shader_model as sm
import shader_model_lighting as sl
import shader_vectors_lighting
import shader_vectors_path
import shader_vectors_sun
from shader_vector_format import (
    CHAINED,
    SMOOTH,
    Draw,
    Module,
    Port,
    Values,
    flat,
    merged,
    one,
    out,
    ports,
    unit,
    word,
)

ROOT = Path(__file__).resolve().parents[1]
VECTORS = ROOT / "shaders" / "tests" / "vectors"
MARGIN = 1e-5
REF_DIGITS = 12


def surface(i: Values, l: str = "l", f0: str = "f0") -> sm.Surface:
    return sm.Surface(sm.vec3(i["n"]), sm.vec3(i["v"]), sm.vec3(i.get(l, [0.0, 0.0, 1.0])), sm.vec3(i["albedo"]),
                   one(i, "metallic"), one(i, "roughness"), sm.vec3(i.get(f0, [0.0, 0.0, 0.0])))


def canonical(module: Module, name: str, raw: Values) -> Values:
    """Each input at the value the kernel reads: an f32 as its shortest decimal, an integer as a 32-bit word."""
    values: Values = {}
    for port in module.inputs:
        given = raw[port.name]
        if len(given) != port.width:
            sys.exit(f"{module.name}: case '{name}' gives {port.name} {len(given)} components, not {port.width}")
        if port.kind == "f32":
            values[port.name] = [sm.shortest_f32(sm.f32(x)) for x in given]
        else:
            values[port.name] = [int(x) for x in given]
    if set(raw) - set(values):
        sys.exit(f"{module.name}: case '{name}' names no input {sorted(set(raw) - set(values))}")
    return values


def strays(port: Port, reference: list[float], rounded: list[float]) -> list[str]:
    """Components where the f32 evaluation misses the reference by more than an eighth of the tolerance."""
    found = []
    for k, (ref, f32_value) in enumerate(zip(reference, rounded, strict=True)):
        if port.kind != "f32":
            if ref != f32_value:
                found.append(f"{port.name}[{k}] f32 run {f32_value} against {ref}")
        elif not abs(f32_value - ref) <= (port.abs + port.rel * abs(ref)) / 8.0:
            found.append(f"{port.name}[{k}] f32 run {f32_value!r} against {ref!r}")
    return found


def build_case(module: Module, name: str, raw: Values) -> tuple[dict[str, object], set[str]]:
    inputs = canonical(module, name, raw)
    with sm.evaluation(float):
        reference = module.evaluate(inputs)
        margins, visits = sm.Model.margins, sm.Model.visits
    with sm.evaluation(sm.f32):
        rounded = module.evaluate(inputs)
    problems = [f"branch '{label}' {margin:.1e} from its edge" for label, margin in margins if margin < MARGIN]
    for port in module.outputs:
        problems += strays(port, reference[port.name], rounded[port.name])
    if problems:
        sys.exit(f"{module.name}: case '{name}' is too close to the f32 limits:\n  " + "\n  ".join(problems))
    outputs = {
        port.name: [float(f"{x:.{REF_DIGITS}g}") if port.kind == "f32" else int(x) for x in reference[port.name]]
        for port in module.outputs
    }
    return {"name": name, "inputs": inputs, "outputs": outputs}, visits


def about(module: Module) -> str:
    return (
        f"Inputs, f64 references and tolerances for tests/{module.name}_test.slang, which runs every public function "
        f"of common/{module.name}.slang once per case. OxidANT's crates/webgpu_renderer/tests/shader_vectors.rs and "
        "BeschleunigerBallett's C++ runner check it. The kernel reads two u32 header words (input words per case, "
        "output words per case), then each case's inputs in `inputs` order, one 32-bit word per component (an f32 "
        "as its IEEE bits, a u32 or i32 as the integer), and writes each case's outputs the same way in `outputs` "
        "order. An f32 output passes when |gpu - ref| <= abs + rel * |ref| in every component; a u32 or i32 output "
        "must be equal. Written by scripts/generate-shader-vectors.py from an f64 model of the module: edit that."
    )


def render(module: Module) -> str:
    cases, reached = [], set()
    for name, raw in module.cases:
        case, visits = build_case(module, name, raw)
        cases.append(case)
        reached |= visits
    if module.visits - reached:
        sys.exit(f"{module.name}: no case reaches {sorted(module.visits - reached)}")
    ports = {
        "inputs": [{"name": p.name, "type": p.kind, "width": p.width} for p in module.inputs],
        "outputs": [
            {"name": p.name, "type": p.kind, "width": p.width} | ({"abs": p.abs, "rel": p.rel} if p.kind == "f32" else {})
            for p in module.outputs
        ],
    }
    lines = ["{", f'  "about": {json.dumps(about(module))},']
    lines += [f'  "{key}": {json.dumps(value)},' for key, value in (
        ("module", f"common/{module.name}.slang"),
        ("kernel", f"tests/{module.name}_test.slang"),
        ("entry", f"{module.name}_test_main"),
    )]
    for key, rows in ports.items():
        lines.append(f'  "{key}": [')
        lines.append(",\n".join(f"    {json.dumps(row)}" for row in rows))
        lines.append("  ],")
    lines.append('  "cases": [')
    lines.append(",\n".join(f"    {json.dumps(case)}" for case in cases))
    lines += ["  ]", "}", ""]
    return "\n".join(lines)


def same(a: object, b: object) -> bool:
    """Equal structure and text, numbers within 1e-9: a libm's last bit must not fail the check."""
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b, strict=True))
    if isinstance(a, bool) or isinstance(b, bool) or not isinstance(a, int | float) or not isinstance(b, int | float):
        return a == b
    return abs(a - b) <= 1e-12 + 1e-9 * abs(b)


def evaluate_brdf(i: Values) -> dict[str, list[float]]:
    s, radiance, k = surface(i), sm.vec3(i["radiance"]), one(i, "k")
    n_dot_v, energy = one(i, "n_dot_v"), sm.Vec(*i["energy"])
    return {
        "lambert_diffuse": flat(sm.lambert_diffuse(s.albedo)),
        "distribution_ggx": flat(sm.distribution_ggx(one(i, "n_dot_h"), s.roughness)),
        "smith_k_analytic": flat(sm.smith_k_analytic(s.roughness)),
        "smith_k_sampled": flat(sm.smith_k_sampled(s.roughness)),
        "geometry_smith_k": flat(sm.geometry_smith_k(one(i, "n_dot_v"), one(i, "n_dot_l"), k)),
        "fresnel_schlick": flat(sm.fresnel_schlick(one(i, "cos_theta"), s.f0)),
        "ggx_single_scatter": flat(sm.ggx_single_scatter(
            n_dot_v, one(i, "n_dot_l"), one(i, "n_dot_h"), one(i, "cos_theta"), s.roughness, s.f0, k)),
        "ggx_energy_node": flat(sm.ggx_energy_node(sm.Vec(*i["t"]))),
        "ggx_energy_coordinates": flat(sm.ggx_energy_coordinates(n_dot_v, s.roughness)),
        "ggx_energy": flat(sm.ggx_energy(n_dot_v, s.roughness)),
        "multiscatter_gain": flat(sm.multiscatter_gain(energy, s.f0)),
        "specular_albedo": flat(sm.specular_albedo(energy, s.f0)),
        "brdf_smith_k": flat(sm.brdf_smith_k(s, radiance, k)),
        "brdf_direct": flat(sm.brdf_direct(s, radiance)),
        "brdf_sampled": flat(sm.brdf_sampled(s)),
    }


def evaluate_noise(i: Values) -> dict[str, list[float]]:
    p, x = sm.vec3(i["p"]), sm.Vec(*i["x"])
    return {
        "mod289.float3": flat(sm.mod289(x.xyz)),
        "mod289.float4": flat(sm.mod289(x)),
        "permute": flat(sm.permute(x)),
        "taylorInvSqrt": flat(sm.taylor_inv_sqrt(x)),
        "snoise": flat(sm.snoise(p)),
        "fbm": flat(sm.fbm(p, one(i, "freq"), word(i, "octaves"))),
    }


def evaluate_path_estimator(i: Values) -> dict[str, list[float]]:
    rng = word(i, "rng")
    drawn, drawn_rng = sm.rng_next(rng)
    alive, kept, roulette_rng = sm.russian_roulette_survives(word(i, "bounce"), sm.vec3(i["throughput"]), rng)
    t, b = sm.orthonormal_basis(sm.vec3(i["n"]))
    direction, weight, bounce_rng = sm.sample_bounce(surface(i, l="none"), sm.vec3(i["face_normal"]), rng)
    n_dot_v, alpha = one(i, "n_dot_v"), one(i, "alpha")
    return {
        "path_rng_seed": flat(sm.path_rng_seed(word(i, "pixel_index"), word(i, "stream"))),
        "rng_next": flat(drawn),
        "rng_next.state": flat(drawn_rng),
        "russian_roulette_survives": flat(alive),
        "russian_roulette_survives.throughput": flat(kept),
        "russian_roulette_survives.rng": flat(roulette_rng),
        "orthonormal_basis.t": flat(t),
        "orthonormal_basis.b": flat(b),
        "sample_ggx_visible_normal": flat(sm.sample_ggx_visible_normal(sm.vec3(i["local_v"]), alpha, sm.Vec(*i["u"]))),
        "ggx_visible_normal_pdf": flat(sm.ggx_visible_normal_pdf(n_dot_v, one(i, "n_dot_h"), alpha)),
        "specular_lobe_probability": flat(
            sm.specular_lobe_probability(sm.vec3(i["albedo"]), one(i, "metallic"), sm.vec3(i["f0"]), n_dot_v)),
        "sample_bounce.direction": flat(direction),
        "sample_bounce.weight": flat(weight),
        "sample_bounce.rng": flat(bounce_rng),
    }


def f32_word(x: float) -> int:
    return struct.unpack("<I", struct.pack("<f", x))[0]


def evaluate_hdr_safe(i: Values) -> dict[str, list[float]]:
    words = [int(w) for w in i["radiance"]]
    return {"hdr_safe": flat(sm.hdr_safe(words)), "is_finite": [float(math.isfinite(sm.bits_to_f32(w))) for w in words]}


def evaluate_aces(i: Values) -> dict[str, list[float]]:
    display = sm.vec3(i["display"])
    return {
        "aces_tonemap": flat(sm.aces_tonemap(sm.vec3(i["radiance"]))),
        "aces_tonemap_inverse": flat(sm.aces_tonemap_inverse(display)),
        "aces_tonemap_inverse.round_trip": flat(sm.aces_tonemap(sm.aces_tonemap_inverse(display))),
        "linear_to_srgb": flat(sm.linear_to_srgb(sm.vec3(i["encoded"]))),
    }


def evaluate_fullscreen(i: Values) -> dict[str, list[float]]:
    position, uv = sm.fullscreen_vs(word(i, "vid"))
    given_uv, given_ndc = sm.Vec(*i["uv"]), sm.Vec(*i["ndc"])
    return {
        "fullscreen_vs.position": flat(position),
        "fullscreen_vs.uv_to_ndc": flat(sm.fullscreen_uv_to_ndc(uv)),
        "fullscreen_vs.ndc_to_uv_error": flat(sm.fullscreen_ndc_to_uv(position.xy) - uv),
        "fullscreen_uv_to_ndc.x": flat(sm.fullscreen_uv_to_ndc(given_uv).x),
        "fullscreen_uv_to_ndc.round_trip": flat(sm.fullscreen_ndc_to_uv(sm.fullscreen_uv_to_ndc(given_uv))),
        "fullscreen_ndc_to_uv.x": flat(sm.fullscreen_ndc_to_uv(given_ndc).x),
        "fullscreen_ndc_to_uv.round_trip": flat(sm.fullscreen_uv_to_ndc(sm.fullscreen_ndc_to_uv(given_ndc))),
        "fullscreen_vs.top_row_error": [0.0],
        "fullscreen_uv_to_ndc.top_row_error": [0.0],
        "fullscreen_ndc_to_uv.top_row_error": [0.0],
        "fullscreen_ndc_uv_y_sign.error": [0.0],
    }


def evaluate_camera_ray(i: Values) -> dict[str, list[float]]:
    rows = [sm.Vec(*i[f"row{k}"]) for k in range(4)]
    origin, direction, length = sm.camera_ray(rows, sm.Vec(*i["ndc"]))
    return {
        "camera_ray.origin": flat(origin),
        "camera_ray.direction": flat(direction),
        "camera_ray.length": flat(length),
        "camera_ray_through_pixel.origin_error": [0.0, 0.0, 0.0],
        "camera_ray_through_pixel.direction_error": [0.0, 0.0, 0.0],
    }


def evaluate_ambient(i: Values) -> dict[str, list[float]]:
    s, energy = surface(i, l="none"), sm.Vec(*i["energy"])
    return {
        "ambient_split_sum": flat(sm.ambient_split_sum(
            sm.vec3(i["irradiance"]), sm.vec3(i["reflection"]), s.albedo, s.metallic, s.f0, energy)),
        "analytic_ambient": flat(sm.analytic_ambient(s, energy)),
        "sky_ambient": flat(sl.sky_ambient(s, word(i, "baked") != 0, energy)),
    }


def evaluate_gltf_material(i: Values) -> dict[str, list[float]]:
    m = [sm.vec3(i[f"m{k}"]) for k in range(3)]
    factor, sampled = one(i, "factor"), one(i, "sampled")
    return {
        "transform_uv": flat(sm.transform_uv(sm.Vec(*i["uv"]), sm.vec3(i["row0"]), sm.vec3(i["row1"]))),
        "normalize_or": flat(sm.normalize_or(sm.vec3(i["v"]), sm.vec3(i["fallback"]))),
        "tangent_space_normal": flat(sm.tangent_space_normal(sm.vec3(i["texel"]), one(i, "scale"))),
        "apply_normal_map": flat(sm.apply_normal_map(
            sm.vec3(i["normal"]), sm.Vec(*i["tangent"]), sm.vec3(i["tangent_normal"]), word(i, "front_face") != 0)),
        "mirror_sign": flat(sm.mirror_sign(m)),
        "normal_transform": flat(*sm.normal_transform(m)),
        "material_metallic": flat(sm.material_metallic(factor, sampled)),
        "material_roughness": flat(sm.material_roughness(factor, sampled)),
        "alpha_masked_out": flat(sm.alpha_masked_out(word(i, "alpha_mode"), one(i, "alpha"), one(i, "cutoff"))),
        "material_occlusion": flat(sm.material_occlusion(factor, sampled)),
    }


def brdf_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "n": [0.0, 1.0, 0.0], "v": [0.0, 1.0, 0.0], "l": unit(1.0, 1.0, 0.0), "albedo": [0.8, 0.2, 0.1],
        "metallic": [0.0], "roughness": [0.5], "f0": [0.04, 0.04, 0.04], "radiance": [1.0, 1.0, 1.0],
        "k": [0.28125], "n_dot_h": [0.9], "n_dot_v": [0.8], "n_dot_l": [0.6], "cos_theta": [0.7],
        "t": [0.3, 0.7], "energy": [0.6, 0.2],
    }
    gold = [1.0, 0.766, 0.336]
    rows: list[tuple[str, Values]] = [
        ("the retired dual-target guard's surface: head-on view, light at 45 degrees", {}),
        ("a gold metal lit from the side, seen at 60 degrees", {
            "v": unit(0.0, 0.5, 0.866), "l": unit(-0.7, 0.5, 0.2), "albedo": gold, "metallic": [1.0],
            "roughness": [0.3], "f0": gold, "radiance": [3.0, 2.5, 2.0], "k": [0.2], "n_dot_h": [0.97],
            "n_dot_v": [0.5], "n_dot_l": [0.35], "cos_theta": [0.95]}),
        ("roughness below the 0.045 floor evaluates as the floor", {"roughness": [0.01], "n_dot_h": [0.9], "k": [0.1]}),
        ("roughness zero evaluates as the floor too", {"roughness": [0.0], "n_dot_h": [0.99]}),
        ("a white metal at roughness zero seen at n.v 0.01: an unfloored remap gave it an albedo of 1.21", {
            "v": unit(0.99995, 0.01, 0.0), "l": unit(-0.6, 0.8, 0.0), "albedo": [1.0, 1.0, 1.0], "metallic": [1.0],
            "roughness": [0.0], "f0": [1.0, 1.0, 1.0], "radiance": [4.0, 4.0, 4.0], "n_dot_v": [0.01]}),
        ("a rough dielectric, roughness one", {"roughness": [1.0], "n_dot_h": [0.3], "cos_theta": [0.1]}),
        ("light below the horizon contributes nothing", {"l": unit(0.3, -0.5, 0.1), "n_dot_l": [0.0]}),
        ("view below the horizon, n.v floored at 1e-4", {"v": unit(0.2, -0.1, 0.9), "n_dot_v": [0.0], "k": [0.0]}),
        ("grazing view and light", {"v": unit(1.0, 0.02, 0.0), "l": unit(-1.0, 0.03, 0.1), "n_dot_v": [0.02]}),
        ("cos above one clamps Fresnel to f0", {"cos_theta": [1.25], "f0": [0.9, 0.5, 0.1]}),
        ("cos below zero clamps Fresnel to one", {"cos_theta": [-0.5], "n_dot_v": [0.0]}),
        ("a coloured light on a half metal", {
            "metallic": [0.5], "f0": [0.42, 0.12, 0.07], "radiance": [0.2, 0.9, 2.5], "v": unit(0.3, 0.8, -0.2)}),
        ("a white metal at roughness 0.7 seen head-on, the furnace's 0.671 single scatter", {
            "albedo": [1.0, 1.0, 1.0], "metallic": [1.0], "roughness": [0.7], "f0": [1.0, 1.0, 1.0], "n_dot_v": [1.0],
            "t": [0.0, 1.0], "energy": [0.67, 0.0]}),
        ("fit coordinates outside [0, 1] clamp to its edges", {"t": [-0.5, 1.5]}),
        ("an energy below the gain's 1e-3 floor", {"energy": [0.0002, 0.0003], "f0": [0.9, 0.5, 0.1]}),
    ]
    draw, fit = Draw(1), Draw(4)
    for index in range(6):
        n = draw.direction()
        rows.append((f"seeded surface {index}", {
            "n": n, "v": draw.direction(n), "l": draw.direction(n), "albedo": draw.nums(0.0, 1.0),
            "metallic": [draw.num(0.0, 1.0)], "roughness": [draw.num(0.05, 1.0)], "f0": draw.nums(0.02, 1.0),
            "radiance": draw.nums(0.0, 4.0), "k": [draw.num(0.0, 0.6)], "n_dot_h": [draw.num(0.0, 0.98)],
            "n_dot_v": [draw.num(0.01, 1.0)], "n_dot_l": [draw.num(0.0, 1.0)], "cos_theta": [draw.num(0.0, 1.0)],
            "t": fit.nums(0.0, 1.0, 2), "energy": [fit.num(0.05, 0.95), fit.num(0.0, 0.1)]}))
    return merged(base, rows)


def noise_cases() -> list[tuple[str, Values]]:
    base: Values = {"p": [0.31, 0.72, 0.18], "x": [0.0, 1.0, 2.0, 3.0], "freq": [1.0], "octaves": [4]}
    rows: list[tuple[str, Values]] = [
        ("the retired guard's point at x 0.1", {"p": [0.1, 0.0, 0.0]}),
        ("the retired guard's point at x 0.7", {"p": [0.7, 0.0, 0.0], "x": [288.0, 289.0, 290.0, 578.0]}),
        ("a lattice point, where the three offsets tie", {"p": [1.0, 2.0, 3.0], "x": [-1.0, -2.5, 12.25, 600.0]}),
        ("inside a cell", {"x": [0.5, 0.75, 1.25, 1.5]}),
        ("negative coordinates at half frequency", {
            "p": [-3.7, 1.25, -0.4], "x": [144.5, 7.0, 48.0, 49.0], "freq": [0.5], "octaves": [3]}),
        ("no octave sums to zero", {"p": [2.2, -1.1, 0.6], "octaves": [0]}),
        ("a negative octave count sums to zero", {"p": [0.9, 0.4, -5.3], "octaves": [-2]}),
        ("far from the origin, one octave", {"p": [57.3, -81.2, 33.9], "octaves": [1], "x": [-300.0, 650.0, 17.0, 5.5]}),
    ]
    draw = Draw(2)
    for index in range(8):
        rows.append((f"seeded point {index}", {
            "p": draw.nums(-6.0, 6.0), "x": [float(draw.whole(-400, 650)) for _ in range(3)] + [draw.num(-4.0, 4.0)],
            "freq": [draw.num(0.25, 1.5)], "octaves": [draw.whole(1, 5)]}))
    return merged(base, rows)


def path_estimator_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "pixel_index": [0], "stream": [3], "rng": [12345], "bounce": [4], "throughput": [0.5, 0.5, 0.5],
        "n": unit(0.2, 1.0, -0.1), "face_normal": [0.0, 1.0, 0.0], "v": unit(0.0, 1.0, 1.0),
        "albedo": [0.8, 0.2, 0.1], "metallic": [0.5], "roughness": [0.4], "f0": [0.42, 0.12, 0.07],
        "local_v": [0.0, 0.6, 0.8], "alpha": [0.16], "u": [0.3, 0.6], "n_dot_v": [0.8], "n_dot_h": [0.95],
    }
    white = [0.9, 0.9, 0.9]
    rows: list[tuple[str, Values]] = [
        ("the retired dual-target guard's surface", {}),
        ("a pixel and a stream that wrap the seed", {"pixel_index": [4000000000], "stream": [7], "rng": [0]}),
        ("the largest rng state", {"rng": [4294967295], "pixel_index": [1920 * 1080 - 1]}),
        ("roulette waits for the fourth segment", {"bounce": [2], "throughput": [0.01, 0.01, 0.01]}),
        ("roulette ends a dim path", {"throughput": [0.01, 0.02, 0.03], "rng": [7]}),
        ("roulette keeps a bright path and reweights it", {"throughput": [2.0, 1.0, 0.5], "rng": [99]}),
        ("a normal facing down takes the other basis", {"n": [0.0, 0.0, -1.0], "face_normal": [0.0, 0.0, -1.0],
                                                       "v": unit(0.3, 0.1, -1.0)}),
        ("a normal in the z = 0 plane", {"n": [1.0, 0.0, 0.0], "face_normal": [1.0, 0.0, 0.0], "v": unit(1.0, 0.5, 0.2)}),
        ("visible normals seen along the normal", {"local_v": [0.0, 0.0, 1.0], "u": [0.7, 0.2]}),
        ("visible normals at a grazing view and a tiny alpha", {"local_v": unit(0.99, 0.0, 0.05), "alpha": [0.002],
                                                               "n_dot_v": [0.05], "n_dot_h": [0.99]}),
        ("visible normals at alpha one", {"local_v": unit(-0.4, 0.3, 0.6), "alpha": [1.0], "u": [0.95, 0.05]}),
        ("a metal bounces into its specular lobe", {"metallic": [1.0], "albedo": [0.95, 0.64, 0.54], "rng": [5]}),
        ("a white dielectric bounces into its diffuse lobe", {"metallic": [0.0], "albedo": white, "rng": [31]}),
        ("a bounce below the face normal ends the path", {"face_normal": [0.0, -1.0, 0.0], "metallic": [0.0],
                                                         "albedo": white, "rng": [31]}),
    ]
    draw = Draw(3)
    for index in range(6):
        n = draw.direction()
        local_v = draw.direction([0.0, 0.0, 1.0])
        rows.append((f"seeded surface {index}", {
            "pixel_index": [draw.whole(0, 1 << 22)], "stream": [draw.whole(0, 64)], "rng": [draw.whole(0, 1 << 32)],
            "bounce": [draw.whole(0, 9)], "throughput": draw.nums(0.0, 1.5), "n": n, "face_normal": n,
            "v": draw.direction(n), "albedo": draw.nums(0.0, 1.0), "metallic": [draw.num(0.0, 1.0)],
            "roughness": [draw.num(0.05, 1.0)], "f0": draw.nums(0.02, 1.0), "local_v": local_v,
            "alpha": [draw.num(0.01, 1.0)], "u": draw.nums(0.0, 1.0, 2), "n_dot_v": [draw.num(0.05, 1.0)],
            "n_dot_h": [draw.num(0.0, 1.0)]}))
    return merged(base, rows)


def hdr_safe_cases() -> list[tuple[str, Values]]:
    nan, inf = 0x7FC00000, 0x7F800000
    rows: list[tuple[str, Values]] = [
        ("ordinary radiance passes unchanged", {"radiance": [f32_word(0.5), f32_word(1.25), f32_word(3.0e4)]}),
        ("a quiet NaN in each channel becomes zero", {"radiance": [nan, f32_word(2.0), nan | 0x80000000]}),
        ("a signalling NaN and the largest NaN payload become zero", {"radiance": [0x7F800001, 0x7FFFFFFF, f32_word(7.0)]}),
        ("+Inf caps at HDR_MAX, -Inf clamps to zero", {"radiance": [inf, inf | 0x80000000, f32_word(1.0)]}),
        ("fp16's 65504 and the largest finite float cap at HDR_MAX", {
            "radiance": [f32_word(65504.0), 0x7F7FFFFF, f32_word(65000.0)]}),
        ("the float just above HDR_MAX caps, the one below passes", {
            "radiance": [f32_word(65000.0) + 1, f32_word(65000.0) - 1, f32_word(64999.0)]}),
        ("negative radiance clamps to zero", {"radiance": [f32_word(-5.0), f32_word(-1.0e-3), f32_word(-1.0e30)]}),
        ("negative zero and the smallest subnormal", {"radiance": [0x80000000, 1, 0]}),
    ]
    draw = Draw(5)
    for index in range(6):
        rows.append((f"seeded radiance {index}", {"radiance": [f32_word(draw.num(0.0, 70000.0)) for _ in range(3)]}))
    return rows


def aces_cases() -> list[tuple[str, Values]]:
    base: Values = {"radiance": [0.18, 0.5, 1.0], "display": [0.2, 0.5, 0.8], "encoded": [0.001, 0.18, 0.9]}
    rows: list[tuple[str, Values]] = [
        ("mid grey, its neighbours and display mid-tones", {}),
        ("zero radiance and a black display", {"radiance": [0.0, 1.0e-4, 0.01], "display": [0.0, 0.01, 0.05]}),
        ("radiance past the curve's shoulder clamps to one", {"radiance": [8.0, 100.0, 1.0e4], "display": [0.95, 0.99, 1.0]}),
        ("negative radiance: below -b/a, about -0.012, the curve turns positive, and -0.5 shows white", {"radiance": [-0.5, -1.0e-3, 2.0]}),
        ("a display outside [0, 1] clamps first", {"display": [-0.25, 1.5, 0.6]}),
        ("the skybox navy, sky.slang's SKYBOX_DISABLED", {"display": [0.0, 0.0, 0.5]}),
        ("encodes on the linear segment", {"encoded": [0.0, 0.002, 0.003]}),
        ("encodes on the power segment, white included", {"encoded": [0.0032, 0.5, 1.0]}),
        ("negative linear values stay on the linear segment", {"encoded": [-0.1, -1.0e-3, 0.05]}),
    ]
    draw = Draw(6)
    for index in range(6):
        rows.append((f"seeded colour {index}", {
            "radiance": draw.nums(0.0, 12.0), "display": draw.nums(0.0, 1.0), "encoded": draw.nums(0.004, 1.2)}))
    return merged(base, rows)


def fullscreen_cases() -> list[tuple[str, Values]]:
    base: Values = {"vid": [0], "uv": [0.25, 0.75], "ndc": [-0.5, 0.5]}
    rows: list[tuple[str, Values]] = [
        ("the first vertex, bottom-left in clip space", {}),
        ("the second vertex, three units up", {"vid": [1], "uv": [0.0, 0.0], "ndc": [-1.0, -1.0]}),
        ("the third vertex, three units right", {"vid": [2], "uv": [1.0, 1.0], "ndc": [1.0, 1.0]}),
        ("the texel centres of a 4 x 4 target's corners", {"uv": [0.125, 0.875], "ndc": [0.75, -0.75]}),
        ("the image centre", {"vid": [1], "uv": [0.5, 0.5], "ndc": [0.0, 0.0]}),
        ("off-image positions the triangle's far vertices reach", {"vid": [2], "uv": [2.0, -1.0], "ndc": [3.0, -1.0]}),
        ("a vertex index past the triangle repeats the pattern", {"vid": [3], "uv": [0.3, 0.1], "ndc": [0.2, -0.9]}),
    ]
    draw = Draw(7)
    for index in range(6):
        rows.append((f"seeded position {index}", {
            "vid": [draw.whole(0, 3)], "uv": draw.nums(0.0, 1.0, 2), "ndc": draw.nums(-1.0, 1.0, 2)}))
    return merged(base, rows)


def matmul(a: list[list[float]], b: list[list[float]]) -> list[list[float]]:
    return [[sum(a[r][k] * b[k][c] for k in range(4)) for c in range(4)] for r in range(4)]


def inverse(m: list[list[float]]) -> list[list[float]]:
    """Gauss-Jordan with partial pivoting, in f64."""
    a = [row[:] + [float(r == c) for c in range(4)] for r, row in enumerate(m)]
    for col in range(4):
        pivot = max(range(col, 4), key=lambda r: abs(a[r][col]))
        a[col], a[pivot] = a[pivot], a[col]
        scale = a[col][col]
        a[col] = [x / scale for x in a[col]]
        for r in range(4):
            if r != col:
                factor = a[r][col]
                a[r] = [x - factor * y for x, y in zip(a[r], a[col], strict=True)]
    return [row[4:] for row in a]


def look_at(eye: list[float], target: list[float]) -> list[list[float]]:
    """glm::lookAt, right-handed, +Y up."""
    f = unit(*(t - e for t, e in zip(target, eye, strict=True)))
    s = unit(f[1] * 0.0 - f[2] * 1.0, f[2] * 0.0 - f[0] * 0.0, f[0] * 1.0 - f[1] * 0.0)
    u = [s[1] * f[2] - s[2] * f[1], s[2] * f[0] - s[0] * f[2], s[0] * f[1] - s[1] * f[0]]
    dots = [sum(a * b for a, b in zip(axis, eye, strict=True)) for axis in (s, u, f)]
    return [s + [-dots[0]], u + [-dots[1]], [-x for x in f] + [dots[2]], [0.0, 0.0, 0.0, 1.0]]


def vulkan_perspective(fov_y: float, aspect: float, near: float, far: float, shift: tuple[float, float] = (0.0, 0.0)):
    """The C++ makeVulkanProjection: glm::perspective at depth [0, 1] with y flipped, then a viewport remap's shift."""
    t = 1.0 / math.tan(math.radians(fov_y) / 2.0)
    return [[t / aspect, 0.0, shift[0], 0.0], [0.0, -t, shift[1], 0.0],
            [0.0, 0.0, far / (near - far), far * near / (near - far)], [0.0, 0.0, -1.0, 0.0]]


def orthographic(half: float, near: float, far: float) -> list[list[float]]:
    return [[1.0 / half, 0.0, 0.0, 0.0], [0.0, 1.0 / half, 0.0, 0.0],
            [0.0, 0.0, 1.0 / (near - far), near / (near - far)], [0.0, 0.0, 0.0, 1.0]]


def clip_to_world(projection: list[list[float]], eye: list[float], target: list[float]) -> Values:
    m = inverse(matmul(projection, look_at(eye, target)))
    return {f"row{k}": m[k] for k in range(4)}


def camera_ray_cases() -> list[tuple[str, Values]]:
    golden = vulkan_perspective(45.0, 1.0, 0.1, 150.0)
    base: Values = clip_to_world(golden, [0.0, 2.0, 6.0], [0.0, 0.5, 0.0]) | {
        "ndc": [0.0, 0.0], "pixel": [32.5, 16.5], "size": [64.0, 64.0]}
    rows: list[tuple[str, Values]] = [
        ("the Debug camera's planes, through the image centre", {}),
        ("a corner of the image", {"ndc": [-1.0, -1.0], "pixel": [0.5, 0.5]}),
        ("the opposite corner", {"ndc": [1.0, 1.0], "pixel": [63.5, 63.5]}),
        ("Sponza's planes, 0.1 to 4000", clip_to_world(vulkan_perspective(60.0, 16.0 / 9.0, 0.1, 4000.0),
                                                     [-8.0, 3.0, 0.5], [4.0, 3.5, -0.5]) | {"ndc": [0.3, -0.4]}),
        ("a wide lens looking straight down", clip_to_world(vulkan_perspective(90.0, 1.5, 0.05, 50.0),
                                                           [0.0, 10.0, 0.0001], [0.0, 0.0, 0.0]) | {"ndc": [0.7, 0.2]}),
        ("a viewport remap's lens shift, the docked editor", clip_to_world(
            vulkan_perspective(50.0, 1.25, 0.1, 200.0, (0.2, -0.1)), [3.0, 1.0, 3.0], [0.0, 1.0, 0.0])
         | {"ndc": [-0.6, 0.5]}),
        ("an orthographic camera starts every ray on the near plane", clip_to_world(
            orthographic(5.0, 0.1, 100.0), [0.0, 0.0, 10.0], [0.0, 0.0, 0.0]) | {"ndc": [0.5, -0.5]}),
        ("a pixel ray through a 1920 x 1080 target", {"pixel": [1919.5, 0.5], "size": [1920.0, 1080.0]}),
    ]
    draw = Draw(8)
    for index in range(6):
        eye = draw.nums(-20.0, 20.0)
        target = [e + d for e, d in zip(eye, draw.direction(), strict=True)]
        projection = vulkan_perspective(draw.num(30.0, 90.0), draw.num(0.75, 2.0), 0.1, draw.num(50.0, 1000.0))
        rows.append((f"seeded camera {index}", clip_to_world(projection, eye, target) | {
            "ndc": draw.nums(-1.0, 1.0, 2), "pixel": [draw.num(0.0, 640.0), draw.num(0.0, 480.0)],
            "size": [640.0, 480.0]}))
    return merged(base, rows)


def ambient_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "n": [0.0, 1.0, 0.0], "v": unit(0.3, 0.8, 0.5), "albedo": [0.8, 0.6, 0.4], "metallic": [0.0],
        "roughness": [0.5], "f0": [0.04, 0.04, 0.04], "energy": [0.6, 0.05], "irradiance": [0.3, 0.4, 0.6],
        "reflection": [0.5, 0.6, 0.7], "baked": [0],
    }
    gold = [1.0, 0.766, 0.336]
    rows: list[tuple[str, Values]] = [
        ("ground, its reflection above the horizon", {}),
        ("a face turned down sees the ground and reflects it", {"n": [0.0, -1.0, 0.0], "v": unit(0.2, -0.7, 0.4)}),
        ("a smooth gold mirror reflects the sky alone, no sun: brdf_direct's lobe carries it", {
            "v": unit(-0.62, 0.72, -0.32), "albedo": gold, "metallic": [1.0], "roughness": [0.1], "f0": gold,
            "energy": [0.9, 0.02]}),
        ("roughness one reflects the irradiance alone", {"roughness": [1.0], "v": unit(-0.45, 0.83, -0.3)}),
        ("a metal takes no diffuse share", {"metallic": [1.0], "f0": [0.9, 0.5, 0.1], "albedo": [0.9, 0.5, 0.1]}),
        ("a grazing view reflects just above the horizon", {"v": unit(1.0, 0.05, 0.0), "roughness": [0.2]}),
        ("the smoothest surface reflects the gradient itself", {"v": unit(-0.5, 0.8, -0.3), "roughness": [0.05]}),
        ("a tilted wall between sky and ground", {"n": unit(0.9, 0.1, -0.4), "v": unit(0.6, -0.2, 0.2)}),
        ("an energy below the gain's 1e-3 floor", {"energy": [0.0002, 0.0003], "f0": [0.9, 0.5, 0.1]}),
        ("baked maps: irradiance at n, the prefiltered map at reflect(-v, n), the table at n.v", {"baked": [1]}),
        ("baked maps under a face turned down", {"baked": [1], "n": [0.0, -1.0, 0.0], "v": unit(0.2, -0.7, 0.4)}),
        ("baked maps seen from behind: n.v clamps to 0", {"baked": [1], "v": unit(0.1, -0.9, 0.2)}),
        ("a rough gold metal from the baked maps", {
            "baked": [1], "albedo": gold, "metallic": [1.0], "roughness": [0.9], "f0": gold}),
    ]
    draw = Draw(9)
    for index in range(6):
        n = draw.direction()
        rows.append((f"seeded surface {index}", {
            "n": n, "v": draw.direction(n), "albedo": draw.nums(0.0, 1.0), "metallic": [draw.num(0.0, 1.0)],
            "roughness": [draw.num(0.05, 1.0)], "f0": draw.nums(0.02, 1.0),
            "energy": [draw.num(0.05, 0.95), draw.num(0.0, 0.1)], "irradiance": draw.nums(0.0, 2.0),
            "reflection": draw.nums(0.0, 4.0), "baked": [index % 2]}))
    return merged(base, rows)


def gltf_material_cases() -> list[tuple[str, Values]]:
    identity = {"m0": [1.0, 0.0, 0.0], "m1": [0.0, 1.0, 0.0], "m2": [0.0, 0.0, 1.0]}
    base: Values = {
        "uv": [0.25, 0.75], "row0": [1.0, 0.0, 0.0], "row1": [0.0, 1.0, 0.0], "v": [0.0, 3.0, 4.0],
        "fallback": [0.0, 1.0, 0.0], "texel": [0.5, 0.5, 1.0], "scale": [1.0], "normal": [0.0, 1.0, 0.0],
        "tangent": [1.0, 0.0, 0.0, 1.0], "tangent_normal": [0.0, 0.0, 1.0], "front_face": [1],
        "factor": [1.0], "sampled": [0.5], "alpha_mode": [0], "alpha": [0.2], "cutoff": [0.5],
    } | identity
    rows: list[tuple[str, Values]] = [
        ("the identity transform and the flat texel", {}),
        ("a KHR_texture_transform's offset, rotation and scale", {
            "uv": [0.3, -1.2], "row0": [0.0, -2.0, 0.5], "row1": [3.0, 0.0, -0.25]}),
        ("a two-channel (BC5) texel rebuilds z from xy", {"texel": [0.7, 0.4, 0.0], "tangent_normal": unit(0.4, -0.2, 0.9)}),
        ("a two-channel texel past the unit circle rebuilds z as 0", {"texel": [1.0, 1.0, 0.0]}),
        ("the normal scale scales x and y alone", {"texel": [0.8, 0.3, 0.9], "scale": [2.0]}),
        ("scale 0 keeps the geometric normal", {"texel": [0.9, 0.1, 0.0], "scale": [0.0]}),
        ("a zero vector takes the fallback", {"v": [0.0, 0.0, 0.0], "fallback": [0.0, 0.0, -1.0]}),
        ("a back face reverses the mapped normal", {"tangent_normal": unit(0.3, 0.2, 0.8), "front_face": [0]}),
        ("a mirrored frame's bitangent follows its handedness", {
            "tangent": [0.0, 0.0, 1.0, -1.0], "tangent_normal": unit(-0.5, 0.4, 0.7), "normal": unit(0.2, 1.0, 0.1)}),
        ("a zero tangent leaves the normal", {"tangent": [0.0, 0.0, 0.0, 1.0], "tangent_normal": unit(0.6, 0.1, 0.7)}),
        ("a tangent along the normal leaves it too", {"tangent": [0.0, 2.0, 0.0, 1.0], "tangent_normal": unit(0.1, 0.6, 0.7)}),
        ("a mirroring matrix: the sign and the cofactor flip", {
            "m0": [-1.0, 0.0, 0.0], "m1": [0.0, 2.0, 0.0], "m2": [0.0, 0.0, 0.5]}),
        ("a non-uniform scale with a shear", {"m0": [2.0, 0.5, 0.0], "m1": [0.0, 1.0, 0.0], "m2": [0.3, 0.0, 4.0]}),
        ("metallic above 1 is held at 1", {"factor": [1.0], "sampled": [1.5]}),
        ("a zero G keeps the roughness floor", {"factor": [0.8], "sampled": [0.0]}),
        ("a MASK alpha below the cutoff is cut out", {"alpha_mode": [1], "alpha": [0.3], "cutoff": [0.5]}),
        ("a MASK alpha above the cutoff stays", {"alpha_mode": [1], "alpha": [0.7], "cutoff": [0.5]}),
        ("BLEND never cuts out, whatever its cutoff", {"alpha_mode": [2], "alpha": [0.1], "cutoff": [0.5]}),
        ("OPAQUE with the -1 cutoff the engine passes", {"alpha_mode": [0], "alpha": [0.0], "cutoff": [-1.0]}),
        ("occlusion strength 0.5 darkens half as far as its texel", {"factor": [0.5], "sampled": [0.2]}),
        ("occlusion strength 0 leaves the ambient whole", {"factor": [0.0], "sampled": [0.0]}),
    ]
    draw = Draw(10)
    for index in range(6):
        n = draw.direction()
        rows.append((f"seeded material {index}", {
            "uv": draw.nums(-2.0, 2.0, 2), "row0": draw.nums(-2.0, 2.0), "row1": draw.nums(-2.0, 2.0),
            "v": draw.nums(-5.0, 5.0), "texel": draw.nums(0.05, 0.95), "scale": [draw.num(0.0, 2.0)], "normal": n,
            "tangent": draw.nums(-1.0, 1.0) + [1.0 if index % 2 else -1.0], "tangent_normal": draw.direction([0.0, 0.0, 1.0]),
            "front_face": [index % 3 != 0], "m0": draw.nums(-3.0, 3.0), "m1": draw.nums(-3.0, 3.0),
            "m2": draw.nums(-3.0, 3.0), "factor": [draw.num(0.0, 1.0)], "sampled": [draw.num(0.0, 1.0)],
            "alpha_mode": [draw.whole(0, 3)], "alpha": [draw.num(0.0, 1.0)], "cutoff": [draw.num(0.1, 0.9)]}))
    return merged(base, rows)


MODULES = [
    Module("brdf", ports(
        "n:3 v:3 l:3 albedo:3 metallic roughness f0:3 radiance:3 k n_dot_h n_dot_v n_dot_l cos_theta t:2 energy:2"), [
        out("lambert_diffuse", 3, SMOOTH), out("distribution_ggx", 1, SMOOTH), out("smith_k_analytic", 1, SMOOTH),
        out("smith_k_sampled", 1, SMOOTH), out("geometry_smith_k", 1, SMOOTH), out("fresnel_schlick", 3, SMOOTH),
        out("ggx_single_scatter", 3, CHAINED), out("ggx_energy_node", 2, SMOOTH),
        out("ggx_energy_coordinates", 2, SMOOTH), out("ggx_energy", 2, CHAINED), out("multiscatter_gain", 3, SMOOTH),
        out("specular_albedo", 3, SMOOTH),
        out("brdf_smith_k", 3, CHAINED), out("brdf_direct", 3, CHAINED), out("brdf_sampled", 3, CHAINED),
    ], evaluate_brdf, brdf_cases(), {
        "roughness floor", "fresnel clamp", "light below the horizon", "fit coordinate clamp", "energy input clamp",
        "gain floor"}),
    Module("noise", ports("p:3 x:4 freq octaves/i32"), [
        out("mod289.float3", 3, (2e-4, 0.0)), out("mod289.float4", 4, (2e-4, 0.0)), out("permute", 4, (1e-3, 0.0)),
        out("taylorInvSqrt", 4, SMOOTH), out("snoise", 1, (2e-5, 0.0)), out("fbm", 1, (1e-4, 0.0)),
    ], evaluate_noise, noise_cases(), {"no octave"}),
    Module("path_estimator", ports(
        "pixel_index/u32 stream/u32 rng/u32 bounce/u32 throughput:3 n:3 face_normal:3 v:3 albedo:3 metallic roughness "
        "f0:3 local_v:3 alpha u:2 n_dot_v n_dot_h"), [
        out("path_rng_seed", 1), out("rng_next", 1, (1e-7, 1e-6)), out("rng_next.state", 1),
        out("russian_roulette_survives", 1), out("russian_roulette_survives.throughput", 3, SMOOTH),
        out("russian_roulette_survives.rng", 1), out("orthonormal_basis.t", 3, SMOOTH),
        out("orthonormal_basis.b", 3, SMOOTH), out("sample_ggx_visible_normal", 3, CHAINED),
        out("ggx_visible_normal_pdf", 1, SMOOTH), out("specular_lobe_probability", 1, SMOOTH),
        out("sample_bounce.direction", 3, CHAINED), out("sample_bounce.weight", 3, CHAINED), out("sample_bounce.rng", 1),
    ], evaluate_path_estimator, path_estimator_cases(), {
        "roulette not yet", "roulette ends the path", "roulette keeps the path", "view along the normal",
        "specular lobe", "diffuse lobe", "the path ends below a surface"}),
    Module("hdr_safe", ports("radiance:3/u32"), [out("hdr_safe", 3, SMOOTH), out("is_finite", 3)], evaluate_hdr_safe, hdr_safe_cases(), {
        "NaN zeroed", "negative zeroed", "infinity capped", "capped at HDR_MAX"}),
    Module("aces", ports("radiance:3 display:3 encoded:3"), [
        out("aces_tonemap", 3, SMOOTH), out("aces_tonemap_inverse", 3, CHAINED),
        out("aces_tonemap_inverse.round_trip", 3, CHAINED), out("linear_to_srgb", 3, CHAINED),
    ], evaluate_aces, aces_cases(), {"tonemap clamp", "display clamp", "linear segment", "power segment"}),
    Module("fullscreen", ports("vid/u32 uv:2 ndc:2"), [
        out("fullscreen_vs.position", 4, SMOOTH), out("fullscreen_vs.uv_to_ndc", 2, CHAINED),
        out("fullscreen_vs.ndc_to_uv_error", 2, CHAINED), out("fullscreen_uv_to_ndc.x", 1, CHAINED),
        out("fullscreen_uv_to_ndc.round_trip", 2, CHAINED), out("fullscreen_ndc_to_uv.x", 1, CHAINED),
        out("fullscreen_ndc_to_uv.round_trip", 2, CHAINED), out("fullscreen_vs.top_row_error", 1, CHAINED),
        out("fullscreen_uv_to_ndc.top_row_error", 1, CHAINED), out("fullscreen_ndc_to_uv.top_row_error", 1, CHAINED),
        out("fullscreen_ndc_uv_y_sign.error", 1, CHAINED),
    ], evaluate_fullscreen, fullscreen_cases()),
    Module("camera_ray", ports("row0:4 row1:4 row2:4 row3:4 ndc:2 pixel:2 size:2"), [
        out("camera_ray.origin", 3, CHAINED), out("camera_ray.direction", 3, CHAINED), out("camera_ray.length", 1, CHAINED),
        out("camera_ray_through_pixel.origin_error", 3, (1e-4, 0.0)),
        out("camera_ray_through_pixel.direction_error", 3, (1e-5, 0.0)),
    ], evaluate_camera_ray, camera_ray_cases()),
    Module("ambient", ports(
        "n:3 v:3 albedo:3 metallic roughness f0:3 energy:2 irradiance:3 reflection:3 baked/u32"), [
        out("ambient_split_sum", 3, SMOOTH), out("analytic_ambient", 3, CHAINED), out("sky_ambient", 3, CHAINED),
    ], evaluate_ambient, ambient_cases(), {"sky side", "ground side", "gain floor", "baked maps", "analytic sky"}),
    Module("gltf_material", ports(
        "uv:2 row0:3 row1:3 v:3 fallback:3 texel:3 scale normal:3 tangent:4 tangent_normal:3 front_face/u32 "
        "m0:3 m1:3 m2:3 factor sampled alpha_mode/i32 alpha cutoff"), [
        out("transform_uv", 2, SMOOTH), out("normalize_or", 3, SMOOTH), out("tangent_space_normal", 3, SMOOTH),
        out("apply_normal_map", 3, CHAINED), out("mirror_sign", 1, SMOOTH), out("normal_transform", 9, CHAINED),
        out("material_metallic", 1, SMOOTH), out("material_roughness", 1, SMOOTH), out("alpha_masked_out", 1),
        out("material_occlusion", 1, SMOOTH),
    ], evaluate_gltf_material, gltf_material_cases(), {
        "two-channel texel", "zero vector", "back face", "mirrored", "metallic clamp", "roughness floor", "not MASK",
        "cut out", "kept"}),
] + shader_vectors_lighting.MODULES + shader_vectors_path.MODULES + shader_vectors_sun.MODULES


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0] if __doc__ else None)
    parser.add_argument("--check", action="store_true", help="fail when a committed file differs from a fresh run")
    args = parser.parse_args()
    stale = []
    for module in MODULES:
        text = render(module)
        path = VECTORS / f"{module.name}.json"
        if not args.check:
            path.write_text(text, encoding="utf-8")
        elif not path.is_file() or not same(json.loads(path.read_text(encoding="utf-8")), json.loads(text)):
            stale.append(str(path.relative_to(ROOT)))
    if stale:
        sys.exit("Regenerate with scripts/generate-shader-vectors.py:\n  " + "\n  ".join(stale))
    print(f"All {len(MODULES)} shader vector files match their models")


if __name__ == "__main__":
    main()
