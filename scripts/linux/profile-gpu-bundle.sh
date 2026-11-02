#!/usr/bin/env bash
# Verify the GPU provider selected by the distributed Linux launcher.
set -euo pipefail
if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo 'usage: profile-gpu-bundle.sh BUNDLE cuda|migraphx PROFILE_PREFIX [LAUNCHER]' >&2
    exit 2
fi
bundle="$1" device="$2" prefix="$3" launcher="${4:-$1/oxidant}"
case "${device}" in cuda|migraphx) ;; *) echo 'unsupported GPU device' >&2; exit 2 ;; esac
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/models-lock.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/models-lock.sh"
model="$(models_lock_file "${repo}/resources/models/models.lock" .onnx)"
test -f "${bundle}/resources/models/${model}" || { echo 'Bundled model missing' >&2; exit 1; }
test -f "${bundle}/lib/libonnxruntime.so" || { echo 'Bundled ONNX Runtime missing' >&2; exit 1; }
hub="${ANTFRASTRUCTURE_DIR:-${repo}/third_party/ANTfrastructure}"
helper="${hub}/linux/scripts/lib/gpu-profile-env.sh"
[[ -f "${helper}" ]] || { echo 'Pinned ANTfrastructure lacks GPU profile environment isolation' >&2; exit 1; }
# shellcheck source=/dev/null
source "${helper}"
declare -F gpu_without_migraphx_overrides >/dev/null || { echo 'Pinned ANTfrastructure lacks GPU profile environment API' >&2; exit 1; }
gpu_without_migraphx_overrides env -u LD_PRELOAD -u LD_AUDIT -u KATAGLYPHIS_ONNX_MODEL -u KATAGLYPHIS_ONNX_BACKEND \
    -u KATAGLYPHIS_ORT_DEVICE -u ORT_DYLIB_PATH -u ORT_LIB_LOCATION "${launcher}" onnx-check --profile "${prefix}"
shopt -s nullglob
profiles=("${prefix}"*.json)
[[ ${#profiles[@]} -eq 1 ]] || { echo 'Expected one ONNX Runtime profile' >&2; exit 1; }
python3 "$(dirname "${BASH_SOURCE[0]}")/../verify-ort-gpu-profile.py" "${profiles[0]}" --device "${device}"
