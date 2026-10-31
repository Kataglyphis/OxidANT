#!/usr/bin/env bash
# Verify the GPU provider selected by the distributed Linux launcher.
set -euo pipefail
if [ "$#" -ne 3 ]; then
    echo 'usage: profile-gpu-bundle.sh BUNDLE cuda|migraphx PROFILE_PREFIX' >&2
    exit 2
fi
bundle="$1" device="$2" prefix="$3"
case "${device}" in cuda|migraphx) ;; *) echo 'unsupported GPU device' >&2; exit 2 ;; esac
test -f "${bundle}/resources/models/deim_dfine_n_coco.onnx" || { echo 'Bundled model missing' >&2; exit 1; }
test -f "${bundle}/lib/libonnxruntime.so" || { echo 'Bundled ONNX Runtime missing' >&2; exit 1; }
env -u KATAGLYPHIS_ONNX_MODEL -u KATAGLYPHIS_ONNX_BACKEND -u KATAGLYPHIS_ORT_DEVICE \
    -u ORT_DYLIB_PATH -u ORT_LIB_LOCATION "${bundle}/oxidant" onnx-check --profile "${prefix}"
shopt -s nullglob
profiles=("${prefix}"*.json)
[[ ${#profiles[@]} -eq 1 ]] || { echo 'Expected one ONNX Runtime profile' >&2; exit 1; }
python3 "$(dirname "${BASH_SOURCE[0]}")/../verify-ort-gpu-profile.py" "${profiles[0]}" --device "${device}"
