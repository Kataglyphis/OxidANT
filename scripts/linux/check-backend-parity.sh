#!/usr/bin/env bash
# ORT on CPU against tract, XNNPACK and DNNL on one real photo through the CLI; GPU providers and packaged launchers are not covered.
set -euo pipefail
# shellcheck source=lib/gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/gpu-check-common.sh"
gpu_check_argc "$#" 2 2 'usage: check-backend-parity.sh LAUNCHER FIXTURE.jpg'
launcher=$(gpu_check_launcher "$1") fixture=$(gpu_check_fixture "$2")
comparator=$(gpu_check_comparator 'Pinned ANTfrastructure lacks the detector parity tools')
profiler=$(gpu_check_profile_tool 'Pinned ANTfrastructure lacks the detector parity tools')
work=$(gpu_check_workspace backend-parity)
trap 'rm -rf -- "$work"' EXIT
clean=(env -u KATAGLYPHIS_ONNX_BACKEND -u KATAGLYPHIS_PREPROCESS -u KATAGLYPHIS_SWAP_XY -u KATAGLYPHIS_SCORE_THRESHOLD)

detect_on() {
  local device=$1 profile
  "${clean[@]}" KATAGLYPHIS_ORT_DEVICE="$device" "$launcher" onnx-detect --image "$fixture" \
    --output "$work/$device.json" --profile "$work/$device-profile" 2>"$work/$device.log" \
    || { cat "$work/$device.log" >&2; return 1; }
  # detect_on also runs as an if condition, where errexit is off and a missing profile must still stop the run.
  profile=$(gpu_check_one_profile "$work/$device-profile" "$device detector") || exit 1
  python3 -I "$profiler" "$profile" --device "$device"
}

detect_on cpu
"${clean[@]}" "$launcher" onnx-detect --tract --image "$fixture" --output "$work/tract.json"
python3 -I "$comparator" "$work/cpu.json" "$work/tract.json"
echo 'ORT and tract agree'
detect_on xnnpack
python3 -I "$comparator" "$work/cpu.json" "$work/xnnpack.json"
echo 'ORT CPU and XNNPACK agree'
if detect_on dnnl; then
  python3 -I "$comparator" "$work/cpu.json" "$work/dnnl.json"
  echo 'ORT CPU and DNNL agree'
else
  grep -q 'ProviderUnavailable: DnnlExecutionProvider' "$work/dnnl.log" \
    || { echo 'DNNL failed without ProviderUnavailable' >&2; exit 1; }
  echo 'DNNL is ProviderUnavailable here'
fi
