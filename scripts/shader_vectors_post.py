"""The post chain's modules' cases and evaluations for generate-shader-vectors.py: exposure."""
import math

import shader_model as sm
import shader_model_post as sp
from shader_vector_format import CHAINED, SMOOTH, Draw, Module, Values, flat, merged, one, out, ports, word


def evaluate_exposure(i: Values) -> dict[str, list[float]]:
    return {
        "exposure_luminance": flat(sp.exposure_luminance(sm.vec3(i["color"]))),
        "histogram_bin": [sp.histogram_bin(one(i, "lum"))],
        "bin_luminance": flat(sp.bin_luminance(word(i, "bin"))),
        "exposure_target_ev": flat(sp.exposure_target_ev(one(i, "log2_mean"))),
        "adapt_exposure_ev": flat(sp.adapt_exposure_ev(
            one(i, "current_ev"), one(i, "target_ev"), one(i, "dt"), one(i, "speed"))),
        "exposure_scale": flat(sp.exposure_scale(one(i, "ev"))),
    }


def exposure_cases() -> list[tuple[str, Values]]:
    base: Values = {
        "color": [0.5, 0.5, 0.5], "lum": [0.18], "bin": [30], "log2_mean": [-2.0], "current_ev": [0.0],
        "target_ev": [2.0], "dt": [1.0 / 60.0], "speed": [3.0], "ev": [0.0],
    }
    rows: list[tuple[str, Values]] = [
        ("middle grey keeps an EV of zero", {"log2_mean": [math.log2(0.18)]}),
        ("a coloured radiance's Rec. 709 luma", {"color": [2.0, 0.5, 0.1], "lum": [0.9]}),
        ("black is bin 0", {"lum": [0.0]}),
        ("a luminance under the black threshold is black", {"lum": [5e-7]}),
        ("a luminance below the range clamps into bin 1", {"lum": [1e-5]}),
        ("a luminance above the range clamps into the last bin", {"lum": [1e5]}),
        ("bin 0 stands for black", {"bin": [0]}),
        ("the last bin's centre", {"bin": [63], "lum": [12.0]}),
        ("a dark scene exposes up", {"log2_mean": [-6.0], "current_ev": [1.0], "target_ev": [4.0]}),
        ("a bright scene exposes down", {"log2_mean": [3.0], "current_ev": [-1.0], "target_ev": [-5.0]}),
        ("a stalled frame holds the current EV", {"dt": [0.0], "current_ev": [1.25]}),
        ("a frame from a clock that ran backwards holds too", {"dt": [-0.5], "current_ev": [-0.75]}),
        ("speed 0 snaps to the target", {"speed": [0.0], "current_ev": [3.0], "target_ev": [-1.5]}),
        ("a long frame lands on the target, never past it", {"dt": [1000.0], "speed": [10.0]}),
        ("an exposure state out of range takes the target", {"current_ev": [1e31], "target_ev": [0.5]}),
        ("an EV of +1 doubles the radiance", {"ev": [1.0]}),
        ("an EV of -1 halves it", {"ev": [-1.0]}),
        ("the editor's EV range ends at +4", {"ev": [4.0], "lum": [3.0e-3]}),
    ]
    draw = Draw(71)
    for index in range(4):
        rows.append((f"seeded frame {index}", {
            "color": draw.nums(0.0, 8.0), "lum": [math.exp2(draw.num(-9.0, 3.5))], "bin": [draw.whole(1, 64)],
            "log2_mean": [draw.num(-9.0, 3.5)], "current_ev": [draw.num(-4.0, 4.0)], "target_ev": [draw.num(-6.0, 6.0)],
            "dt": [draw.num(0.005, 0.1)], "speed": [draw.num(0.1, 10.0)], "ev": [draw.num(-4.0, 4.0)]}))
    return merged(base, rows)


MODULES = [
    Module("exposure", ports(
        "color:3 lum bin/u32 log2_mean current_ev target_ev dt speed ev"), [
        out("exposure_luminance", 1, SMOOTH), out("histogram_bin", 1), out("bin_luminance", 1, CHAINED),
        out("exposure_target_ev", 1, CHAINED), out("adapt_exposure_ev", 1, CHAINED), out("exposure_scale", 1, SMOOTH),
    ], evaluate_exposure, exposure_cases(), {
        "black bin", "below the range", "above the range", "bin 0", "current replaced", "held", "snapped",
        "blend saturated"}),
]
