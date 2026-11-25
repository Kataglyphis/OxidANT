#!/usr/bin/env bash
# check-shared-shader-copies.py on fixture trees: the owner may declare, a private copy elsewhere fails by file and line.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
gate="${tests_dir}/../../check-shared-shader-copies.py"
tree="${work}/shaders"
mkdir -p "${tree}/common" "${tree}/pass"
printf 'static const float HDR_MAX = 65000.0;\nfloat3 hdr_safe(float3 r) { return min(r, float3(HDR_MAX)); }\n' \
    > "${tree}/common/hdr_safe.slang"
printf 'float2 fullscreen_uv_to_ndc(float2 uv) { return uv * 2.0 - 1.0; }\n' > "${tree}/common/fullscreen.slang"

# pass <name> <body>: one pass file holding <body>.
pass() {
    rm -f "${tree}"/pass/*.slang
    printf '%b\n' "$2" > "${tree}/pass/$1.slang"
}

pass clean 'import hdr_safe;\nfloat4 f(float3 c) { return float4(hdr_safe(c), 1.0); }'
t_case "an importer that calls the shared function passes"
t_assert_rc 0 python3 "$gate" "${tree}"

pass commented '// a private HDR_MAX = 65000.0 named in a comment\nfloat x = 1.0;'
t_case "a copy inside a comment passes"
t_assert_rc 0 python3 "$gate" "${tree}"

pass private 'float a = 1.0;\nstatic const float TRACE_HDR_MAX = 65000.0;'
t_case "a renamed private constant fails by file and line"
t_assert_rc 1 python3 "$gate" "${tree}"
python3 "$gate" "${tree}" > "${work}/out" 2>&1
t_assert_grep "${work}/out" '^  pass/private.slang:2: an HDR maximum'

pass limit 'static const int CASCADE_COUNT = 3;'
t_case "a private copy of a limits.slang limit fails"
t_assert_rc 1 python3 "$gate" "${tree}"
python3 "$gate" "${tree}" > "${work}/out" 2>&1
t_assert_grep "${work}/out" '^  pass/limit.slang:1: a cascade or PCF limit'

pass literal 'float3 f(float3 c) { return min(c, float3(65000.0)); }'
t_case "a private HDR maximum literal fails"
t_assert_rc 1 python3 "$gate" "${tree}"

pass selfcompare 'bool ok(float ev) { return ev == ev && abs(ev) < 1e30; }'
t_case "a NaN check by self-comparison fails"
t_assert_rc 1 python3 "$gate" "${tree}"

pass compare 'bool same(float a, float b) { return a == b && a.x != b.x; }'
t_case "comparing two different values passes"
t_assert_rc 0 python3 "$gate" "${tree}"

pass redefined 'float3 hdr_safe(float3 r)\n{\n    return r;\n}'
t_case "a second definition of a shared function fails"
t_assert_rc 1 python3 "$gate" "${tree}"

pass ambient 'import brdf;\nfloat3 analytic_ambient(float3 n, float3 v)\n{\n    return n;\n}'
t_case "a private analytic ambient fails by file and line"
t_assert_rc 1 python3 "$gate" "${tree}"
python3 "$gate" "${tree}" > "${work}/out" 2>&1
t_assert_grep "${work}/out" '^  pass/ambient.slang:2: an ambient term'

pass formula 'float2 to_ndc(float2 uv) { return uv * 2.0 - 1.0; }'
t_case "a uv conversion outside a fullscreen importer passes"
t_assert_rc 0 python3 "$gate" "${tree}"

pass unproject 'float3 f(float2 ndc) { float4 n = mul(frame.inv_view_proj, float4(ndc, 0.0, 1.0)); return n.xyz / n.w; }'
t_case "a near-plane unprojection outside camera_ray.slang fails"
t_assert_rc 1 python3 "$gate" "${tree}"

pass unproject2 'float3 f(float2 ndc) {\n    float4 clip = float4(ndc, 1.0, 1.0);\n    float4 eye = mul(frame.inv_projection, clip);\n    return eye.xyz;\n}'
t_case "a far-plane point unprojected a statement later fails by file and line"
t_assert_rc 1 python3 "$gate" "${tree}"
python3 "$gate" "${tree}" > "${work}/out" 2>&1
t_assert_grep "${work}/out" '^  pass/unproject2.slang:2: a clip-space point on the near or far plane'

pass colour 'float4 red() { float4 c = float4(1.0, 0.0, 0.0, 1.0); return c; }'
t_case "a literal colour with z 0 and w 1 passes"
t_assert_rc 0 python3 "$gate" "${tree}"

pass eye 'float3 f() { return frame.inv_view[3].xyz; }'
t_case "the eye read from the inverse view's last row fails"
t_assert_rc 1 python3 "$gate" "${tree}"

pass importer 'import fullscreen;\nfloat2 to_uv(float2 ndc) { return float2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5); }'
t_case "a fullscreen importer converting by hand fails"
t_assert_rc 1 python3 "$gate" "${tree}"

pass triangle 'import fullscreen;\nFullscreenVsOut vs(uint vid) { FullscreenVsOut o; o.svPosition = float4(float(vid), 0.0, 0.0, 1.0); return o; }'
t_case "a fullscreen importer writing its own triangle fails"
t_assert_rc 1 python3 "$gate" "${tree}"

t_summary
