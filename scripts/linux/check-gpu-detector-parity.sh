#!/usr/bin/env bash
# Compare real-image detections from this package's CPU and GPU launchers.
set -euo pipefail
# shellcheck source=lib/gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/gpu-check-common.sh"
gpu_check_argc "$#" 3 3 'usage: check-gpu-detector-parity.sh LAUNCHER cuda|migraphx FIXTURE.jpg'
launcher=$(gpu_check_launcher "$1") device=$2 fixture=$(gpu_check_fixture "$3")
gpu_check_require_device "$device"
comparator=$(gpu_check_comparator 'Pinned ANTfrastructure lacks detector parity comparison')
intervals=$(gpu_check_profile_tool 'Pinned ANTfrastructure lacks per-run provider profiling')
gpu_check_load_profile_env
work=$(gpu_check_workspace detector-parity)
trap 'rm -rf -- "$work"' EXIT

for selected in cpu "$device"; do
  read -r -a clean <<< "$(gpu_check_clean_env_args)"
  clean+=(-u KATAGLYPHIS_PREPROCESS -u KATAGLYPHIS_SWAP_XY -u KATAGLYPHIS_ORT_THREADS -u KATAGLYPHIS_ORT_SPIN -u KATAGLYPHIS_SCORE_THRESHOLD)
  if [[ "$selected" == cpu ]]; then clean+=(-u LD_LIBRARY_PATH); fi
  gpu_without_migraphx_overrides "${clean[@]}" KATAGLYPHIS_ORT_DEVICE="$selected" "$launcher" onnx-detect \
    --image "$fixture" --output "$work/$selected.json" --profile "$work/$selected-profile" \
    --repeat-output "$work/$selected-repeat.json"
  profile=$(gpu_check_one_profile "$work/$selected-profile" "$selected detector")
  python3 -I "$intervals" "$profile" --device "$selected" --min-runs 2
done
python3 -I "$comparator" "$work/cpu.json" "$work/cpu-repeat.json"
python3 -I "$comparator" "$work/cpu.json" "$work/$device.json"
python3 -I "$comparator" "$work/cpu.json" "$work/$device-repeat.json"
python3 -I "$comparator" --pairwise "$work/cpu-repeat.json" "$work/$device.json"
python3 -I "$comparator" --pairwise "$work/cpu-repeat.json" "$work/$device-repeat.json"
python3 -I "$comparator" --pairwise "$work/$device.json" "$work/$device-repeat.json"
