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
    return rf"^[ \t]*(?!return\b)\w+\s+(?:{names})\s*(?:<[^>(]*>)?\s*\("


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
    ("common/exposure.slang", declares(r"HISTOGRAM_BINS|\w*LOG_LUMINANCE|BLACK_THRESHOLD|EXPOSURE_KEY"),
     "an exposure constant"),
    ("common/exposure.slang", defines(
        "exposure_luminance|histogram_bin|bin_luminance|exposure_target_ev|adapt_exposure_ev|exposure_scale"),
     "an exposure rule"),
    ("common/exposure.slang", r"\bexp2\(\s*exposureState\b", "an exposure scale"),
    ("common/bloom_filter.slang", declares(r"BLOOM_\w+"), "a bloom constant"),
    ("common/bloom_filter.slang", defines("bloom_bright|bloom_tap_weight"), "a bloom rule"),
    ("common/bloom_filter.slang", r"\b0\.227027\b|\b0\.1945946\b", "a bloom tap weight"),
    ("common/aces.slang", defines("aces_tonemap|aces_tonemap_inverse|untonemapped|linear_to_srgb"), "an ACES or sRGB curve"),
    ("common/trace_constants.slang", declares(r"\w*TMIN|\w*TMAX|\w*SURFACE_OFFSET|\w*LIGHT_SPAN"), "a ray bound"),
    ("common/trace_constants.slang", r"(?i)\btm(?:in|ax)\s*=\s*[\d.]*[1-9]", "a literal ray bound"),
    ("common/trace_constants.slang", r"\b10000\.0\b|\b0\.0001\s*\*|\*\s*0\.0001\b|\b1\.0\s*-\s*1e-4\b", "a ray-bound literal"),
    ("common/fullscreen.slang", defines("fullscreen_vs|fullscreen_uv_to_ndc|fullscreen_ndc_to_uv|fullscreen_ndc_uv_y_sign"),
     "a fullscreen mapping"),
    ("common/fullscreen.slang", r"\b0\.5\s*-\s*[\w.\[\]]+\.y\s*\*\s*0\.5\b|\.xy\s*\*\s*0\.5\s*\+\s*0\.5\b",
     "an NDC-to-uv conversion"),
    ("common/camera_ray.slang", defines("camera_ray|camera_ray_through_pixel"), "a camera ray"),
    ("common/camera_ray.slang", r"mul\(\s*[\w.]*(?:inv_view_proj|inv_projection|clip_to_world)\s*,\s*float4\([^;]*,\s*[01](?:\.0)?\s*,\s*1(?:\.0)?\s*\)\s*\)",
     "a near- or far-plane unprojection"),
    ("common/camera_ray.slang", r"float4\s+\w+\s*=\s*float4\(\s*(?![-\d.]+\s*,)[^;]*,\s*[01](?:\.0)?\s*,\s*1(?:\.0)?\s*\)\s*;",
     "a clip-space point on the near or far plane, unprojected a statement later"),
    ("common/camera_ray.slang",
     r"\binv_view\s*,\s*float4\(\s*0(?:\.0)?\s*,\s*0(?:\.0)?\s*,\s*0(?:\.0)?\s*,\s*1(?:\.0)?\s*\)|\binv_view\s*\[\s*3\s*\]",
     "the eye as a ray origin"),
    ("common/ambient.slang", defines("ambient_split_sum|analytic_ambient|sky_ambient"), "an ambient term"),
    ("common/limits.slang", declares(r"CASCADE_COUNT|MAX_PCF_RADIUS"), "a cascade or PCF limit"),
    ("common/alpha_mode.slang", declares(r"ALPHA_OPAQUE|ALPHA_MASK|ALPHA_BLEND"), "an alpha mode"),
    ("common/gltf_material.slang", defines(
        "transform_uv|normalize_or|tangent_space_normal|apply_normal_map|mirror_sign|normal_transform"
        "|material_metallic|material_roughness|alpha_masked_out|material_occlusion"), "a glTF material rule"),
    ("common/gltf_material.slang", r"\blerp\(\s*1\.0\s*,\s*[\w.]+\.r\s*,", "an occlusion rule"),
    ("common/gltf_material.slang", r"\bsqrt\(\s*saturate\(\s*1\.0\s*-\s*dot\(", "a BC5 z rebuild"),
    ("common/gltf_material.slang",
     r"\bdot\(\s*float3\(\s*[\w.]+\s*,\s*1\.0\s*\)|\.x\s*\*\s*\w+\.x\s*\+\s*\w+\.y\s*\*\s*\w+\.y\s*\+\s*\w+\.z\b",
     "a texture transform"),
    ("common/gltf_material.slang", r"\bfloat3x3\(\s*t\s*,\s*b\s*,", "a tangent frame"),
    ("common/cascade_visibility.slang",
     defines("select_cascade|cascade_hit|cascade_bias|cascade_depth_slope|cascade_visibility"),
     "a cascade lookup"),
    ("common/cascade_visibility.slang", r"\bmax\(\s*0\.002\s*\*", "the cascades' depth bias"),
    ("common/punctual_shadow.slang", defines(
        "unpack_punctual_shadow|fetch_punctual_shadow|punctual_shadow_depth|punctual_shadow_face"
        "|punctual_shadow_face_coord|punctual_shadow_visibility"), "a punctual shadow lookup"),
    ("common/punctual_shadow.slang", r"\bstruct\s+PunctualShadow\b", "the punctual shadow record"),
    ("common/surface_lighting.slang", defines("surface_f0|surface_lighting|screen_space_occlusion"), "a surface lighting rule"),
    ("common/surface_lighting.slang", r"\bstruct\s+SurfacePoint\b", "the surface point"),
    ("common/surface_lighting.slang", r"\blerp\(\s*float3\(\s*0\.04\s*\)", "a dielectric f0"),
    ("common/gbuffer.slang", defines("gbuffer_pack|gbuffer_unlit|gbuffer_unpack"), "a G-buffer packing"),
    ("common/gbuffer.slang", r"\.w\s*<\s*-0\.5\b", "the G-buffer's unlit sign"),
    ("common/cloud_upsample.slang", defines("half_texel_depth|cloud_upsample"), "a cloud upsample"),
    ("common/drawn_sky.slang", defines("drawn_sky"), "the drawn sky"),
    ("common/drawn_sky.slang", declares(r"SKYBOX_DISABLED\w*"), "the switched-off sky's navy"),
    ("common/path_integrator.slang", defines(
        "trace_path|next_event|path_miss|furnace_surface|sun_sample|punctual_sample|picked_punctual_sample|shadowed"),
     "a path tracer step"),
    ("common/path_integrator.slang", r"\bstruct\s+(?:TracedSurface|TracedSegment|LightSample|PathDepth)\b",
     "the path tracer's surface, segment, light or depth record"),
    ("common/sun_mask.slang", defines("find_sun|sun_masked"), "the IBL bake's sun search or mask"),
    ("common/sun_mask.slang", r"\bstruct\s+SunMask\b", "the IBL bake's sun mask"),
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
