#!/usr/bin/env bash
# Compare real-image detections from this package's CPU and GPU launchers.
set -euo pipefail
if [[ $# -ne 3 ]]; then echo 'usage: check-gpu-detector-parity.sh LAUNCHER cuda|migraphx FIXTURE.jpg' >&2; exit 2; fi
launcher=$(realpath -e "$1") device=$2 fixture=$(realpath -e "$3")
case "$device" in cuda|migraphx) ;; *) echo 'unsupported GPU device' >&2; exit 2 ;; esac
[[ -x "$launcher" && -f "$fixture" ]] || { echo 'missing launcher or fixture' >&2; exit 1; }
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
hub="${ANTFRASTRUCTURE_DIR:-$repo/third_party/ANTfrastructure}"
comparator="$hub/shared/python/compare_detector_parity.py"
[[ -f "$comparator" ]] || { echo 'Pinned ANTfrastructure lacks detector parity comparison' >&2; exit 1; }
intervals="$hub/shared/python/verify_ort_profile.py"
[[ -f "$intervals" ]] || { echo 'Pinned ANTfrastructure lacks per-run provider profiling' >&2; exit 1; }
helper="$hub/linux/scripts/lib/gpu-profile-env.sh"
[[ -f "$helper" ]] || { echo 'Pinned ANTfrastructure lacks GPU profile environment isolation' >&2; exit 1; }
# shellcheck source=/dev/null
source "$helper"
declare -F gpu_without_migraphx_overrides >/dev/null || { echo 'Pinned ANTfrastructure lacks GPU profile environment API' >&2; exit 1; }
work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/oxidant-detector-parity.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT

for selected in cpu "$device"; do
  clean=(env -u LD_PRELOAD -u LD_AUDIT -u KATAGLYPHIS_ONNX_MODEL -u KATAGLYPHIS_ONNX_BACKEND -u ORT_DYLIB_PATH -u ORT_LIB_LOCATION
    -u KATAGLYPHIS_PREPROCESS -u KATAGLYPHIS_SWAP_XY -u KATAGLYPHIS_ORT_THREADS -u KATAGLYPHIS_ORT_SPIN -u KATAGLYPHIS_SCORE_THRESHOLD)
  if [[ "$selected" == cpu ]]; then clean+=(-u LD_LIBRARY_PATH); fi
  gpu_without_migraphx_overrides "${clean[@]}" KATAGLYPHIS_ORT_DEVICE="$selected" "$launcher" onnx-detect \
    --image "$fixture" --output "$work/$selected.json" --profile "$work/$selected-profile" \
    --repeat-output "$work/$selected-repeat.json"
  shopt -s nullglob
  profiles=("$work/$selected-profile"*.json)
  [[ ${#profiles[@]} -eq 1 ]] || { echo "Expected one $selected detector profile" >&2; exit 1; }
  python3 -I "$intervals" "${profiles[0]}" --device "$selected" --min-runs 2
done
python3 -I "$comparator" "$work/cpu.json" "$work/cpu-repeat.json"
python3 -I "$comparator" "$work/cpu.json" "$work/$device.json"
python3 -I "$comparator" "$work/cpu.json" "$work/$device-repeat.json"
python3 -I "$comparator" --pairwise "$work/cpu-repeat.json" "$work/$device.json"
python3 -I "$comparator" --pairwise "$work/cpu-repeat.json" "$work/$device-repeat.json"
python3 -I "$comparator" --pairwise "$work/$device.json" "$work/$device-repeat.json"
