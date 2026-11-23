#!/usr/bin/env python3
"""Fail on a Slang file that keeps a private copy of a constant or function a shared module owns."""
import argparse
import re
import sys
from pathlib import Path

COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.DOTALL)


def declares(names):
    return rf"\bstatic\s+const\s+\w+\s+(?:{names})\s*="


def defines(names):
    return rf"^[ \t]*(?!return\b)\w+\s+(?:{names})\s*\("


# (owner, pattern, what a match copies); the owner alone may match.
RULES = [
    ("common/constants.slang", declares(r"\w*PI|MIN_\w*ROUGHNESS"), "a PI or roughness-floor constant"),
    ("common/constants.slang", r"\b3\.14159|\b6\.28318|\b0\.31830|\b1\.57079", "a PI literal"),
    ("common/constants.slang", r"(?<![\d.])0\.045(?!\d)", "the 0.045 roughness floor"),
    ("common/hdr_safe.slang", declares(r"\w*HDR_MAX"), "an HDR maximum"),
    ("common/hdr_safe.slang", r"\b65(?:000|504)\b", "the HDR maximum literal"),
    ("common/hdr_safe.slang", r"\bisnan\s*\(|0x7[fF]800000|(?<![\w.])([A-Za-z_]\w*(?:\.\w+|\[\w+\])*)\s*[!=]=\s*\1(?![\w.\[(])",
     "a NaN check"),
    ("common/hdr_safe.slang", defines("hdr_safe"), "hdr_safe"),
    ("common/aces.slang", r"\b2\.51\b|\b2\.43\b", "an ACES coefficient"),
    ("common/aces.slang", defines("aces_tonemap|aces_tonemap_inverse|untonemapped|linear_to_srgb"), "an ACES or sRGB curve"),
    ("common/trace_constants.slang", declares(r"\w*TMIN|\w*TMAX|\w*SURFACE_OFFSET|\w*LIGHT_SPAN"), "a ray bound"),
    ("common/trace_constants.slang", r"(?i)\btm(?:in|ax)\s*=\s*[\d.]*[1-9]", "a literal ray bound"),
    ("common/trace_constants.slang", r"\b10000\.0\b|\b0\.0001\s*\*|\*\s*0\.0001\b|\b1\.0\s*-\s*1e-4\b", "a ray-bound literal"),
    ("common/fullscreen.slang", defines("fullscreen_vs|fullscreen_uv_to_ndc|fullscreen_ndc_to_uv"), "a fullscreen mapping"),
    ("common/camera_ray.slang", defines("camera_ray|camera_ray_through_pixel"), "a camera ray"),
    ("common/camera_ray.slang", r"mul\(\s*[\w.]*(?:inv_view_proj|inv_projection|clip_to_world)\s*,\s*float4\([^;]*,\s*[01](?:\.0)?\s*,\s*1(?:\.0)?\s*\)\s*\)",
     "a near- or far-plane unprojection"),
    ("common/camera_ray.slang", r"float4\s+\w+\s*=\s*float4\(\s*(?![-\d.]+\s*,)[^;]*,\s*[01](?:\.0)?\s*,\s*1(?:\.0)?\s*\)\s*;",
     "a clip-space point on the near or far plane, unprojected a statement later"),
    ("common/camera_ray.slang",
     r"\binv_view\s*,\s*float4\(\s*0(?:\.0)?\s*,\s*0(?:\.0)?\s*,\s*0(?:\.0)?\s*,\s*1(?:\.0)?\s*\)|\binv_view\s*\[\s*3\s*\]",
     "the eye as a ray origin"),
    ("common/ambient.slang", defines("ambient_split_sum|analytic_ambient"), "an ambient term"),
]
# A fullscreen_vs importer converts between uv and NDC through fullscreen.slang only.
FULLSCREEN_COPIES = re.compile(
    r"\(y \+ 1\.0\) \* 0\.5|\buv \* 2\.0 - 1\.0|\buv\.y \* 2\.0|\b0\.5\s*-\s*[^;,]*\*\s*0\.5\b|\.svPosition\s*=\s*float4\(")
IMPORTS_FULLSCREEN = re.compile(r"^\s*import\s+fullscreen\s*;", re.MULTILINE)


def copies(root):
    found = []
    for path in sorted(root.rglob("*.slang")):
        relative = path.relative_to(root).as_posix()
        text = COMMENT.sub(lambda m: re.sub(r"[^\n]", " ", m.group()), path.read_text(encoding="utf-8"))
        rules = [(re.compile(p, re.MULTILINE), what) for owner, p, what in RULES if owner != relative]
        if IMPORTS_FULLSCREEN.search(text):
            rules.append((FULLSCREEN_COPIES, "a uv/NDC conversion or a triangle fullscreen.slang owns"))
        for pattern, what in rules:
            for match in pattern.finditer(text):
                line = text.count("\n", 0, match.start()) + 1
                found.append(f"{relative}:{line}: {what}: {match.group().strip()}")
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, nargs="?", default=Path(__file__).resolve().parents[1] / "shaders")
    root = parser.parse_args().root.resolve()
    found = copies(root)
    if found:
        sys.exit("Import the shared module instead of copying it:\n  " + "\n  ".join(found))
    print(f"No Slang file under {root} copies a shared module")


if __name__ == "__main__":
    main()
