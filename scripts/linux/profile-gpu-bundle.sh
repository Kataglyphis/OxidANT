#!/usr/bin/env bash
# Verify the GPU provider selected by the distributed Linux launcher.
set -euo pipefail
# shellcheck source=lib/gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/gpu-check-common.sh"
gpu_check_argc "$#" 3 4 'usage: profile-gpu-bundle.sh BUNDLE cuda|migraphx PROFILE_PREFIX [LAUNCHER]'
bundle="$1" device="$2" prefix="$3" launcher="${4:-$1/oxidant}"
gpu_check_require_device "${device}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/models-lock.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/models-lock.sh"
model="$(models_lock_file "${repo}/resources/models/models.lock" .onnx)"
test -f "${bundle}/resources/models/${model}" || { echo 'Bundled model missing' >&2; exit 1; }
test -f "${bundle}/lib/libonnxruntime.so" || { echo 'Bundled ONNX Runtime missing' >&2; exit 1; }
gpu_check_load_profile_env
read -r -a clean <<< "$(gpu_check_clean_env_args)"
gpu_without_migraphx_overrides "${clean[@]}" -u KATAGLYPHIS_ORT_DEVICE "${launcher}" onnx-check --profile "${prefix}"
profile="$(gpu_check_one_profile "${prefix}" 'ONNX Runtime')"
python3 "$(dirname "${BASH_SOURCE[0]}")/../verify-ort-gpu-profile.py" "${profile}" --device "${device}"
