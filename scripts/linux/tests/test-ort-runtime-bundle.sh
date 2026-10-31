#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${here}/../lib/ort-runtime-bundle.sh"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
mkdir -p "${work}/ort"
for file in libonnxruntime.so libonnxruntime.so.1 libonnxruntime_providers_shared.so libonnxruntime_providers_cuda.so libonnxruntime_providers_migraphx.so; do
    printf '%s\n' "${file}" > "${work}/ort/${file}"
done
for variant in cpu nvidia amd; do
    stage_ort_runtime "${work}/${variant}" "${work}/ort" "${variant}"
    cmp "${work}/ort/libonnxruntime.so" "${work}/${variant}/libonnxruntime.so"
done
[ ! -e "${work}/cpu/libonnxruntime_providers_shared.so" ]
[ -f "${work}/nvidia/libonnxruntime_providers_cuda.so" ]
[ ! -e "${work}/nvidia/libonnxruntime_providers_migraphx.so" ]
[ -f "${work}/amd/libonnxruntime_providers_migraphx.so" ]
[ ! -e "${work}/amd/libonnxruntime_providers_cuda.so" ]
rm "${work}/ort/libonnxruntime_providers_cuda.so"
if stage_ort_runtime "${work}/missing" "${work}/ort" nvidia 2>"${work}/error"; then
    echo 'missing provider unexpectedly succeeded' >&2; exit 1
fi
[ ! -e "${work}/missing" ]
if stage_ort_runtime "${work}/unknown" "${work}/ort" other 2>/dev/null; then
    echo 'unknown variant unexpectedly succeeded' >&2; exit 1
fi
if bash "${here}/../package-linux.sh" --binary-path /bin/true --version 1.0.0 --arch aarch64 \
    --ort-variant amd --package-types tar --out "${work}/unsupported" 2>"${work}/error"; then
    echo 'Linux arm64 ROCm unexpectedly succeeded' >&2; exit 1
fi
grep -Fq 'Linux arm64 ROCm GPU packages are not supported' "${work}/error"
[ ! -e "${work}/unsupported" ]
cat > "${work}/launcher" <<'LAUNCHER'
#!/bin/bash
here=/fake
printf 'device=%s\n' "$KATAGLYPHIS_ORT_DEVICE"
printf 'search=%s\n' "$LD_LIBRARY_PATH"
printf '%s\n' '--library-path "$here/lib/glibc:$here/lib"'
LAUNCHER
configure_gpu_launcher "${work}/launcher" cuda
env -u KATAGLYPHIS_ORT_DEVICE bash "${work}/launcher" > "${work}/out"
grep -q '^device=cuda$' "${work}/out"
KATAGLYPHIS_ORT_DEVICE=cpu bash "${work}/launcher" > "${work}/out"
grep -q '^device=cpu$' "${work}/out"
LD_LIBRARY_PATH='::/sdk::/driver:' bash "${work}/launcher" > "${work}/out"
grep -q '^search=/fake/lib:/sdk:/driver$' "${work}/out"
# shellcheck disable=SC2016
grep -Fq -- 'lib${gpu_library_path}' "${work}/launcher"
printf 'ORT provider staging tests passed\n'
