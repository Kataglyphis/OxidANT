#!/usr/bin/env bash
# ORT on CPU against tract, XNNPACK and DNNL on one real photo through the CLI; GPU providers and packaged launchers are not covered.
set -euo pipefail
if [[ $# -ne 2 ]]; then echo 'usage: check-backend-parity.sh LAUNCHER FIXTURE.jpg' >&2; exit 2; fi
launcher=$(realpath -e "$1") fixture=$(realpath -e "$2")
[[ -x "$launcher" && -f "$fixture" ]] || { echo 'missing launcher or fixture' >&2; exit 1; }
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
hub="${ANTFRASTRUCTURE_DIR:-$repo/third_party/ANTfrastructure}"
comparator="$hub/shared/python/compare_detector_parity.py"
profiler="$hub/shared/python/verify_ort_profile.py"
[[ -f "$comparator" && -f "$profiler" ]] || { echo 'Pinned ANTfrastructure lacks the detector parity tools' >&2; exit 1; }
work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/oxidant-backend-parity.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
clean=(env -u KATAGLYPHIS_ONNX_BACKEND -u KATAGLYPHIS_PREPROCESS -u KATAGLYPHIS_SWAP_XY -u KATAGLYPHIS_SCORE_THRESHOLD)

detect_on() {
  local device=$1 profiles
  "${clean[@]}" KATAGLYPHIS_ORT_DEVICE="$device" "$launcher" onnx-detect --image "$fixture" \
    --output "$work/$device.json" --profile "$work/$device-profile" 2>"$work/$device.log" \
    || { cat "$work/$device.log" >&2; return 1; }
  shopt -s nullglob
  profiles=("$work/$device-profile"*.json)
  shopt -u nullglob
  [[ ${#profiles[@]} -eq 1 ]] || { echo "Expected one $device detector profile" >&2; exit 1; }
  python3 -I "$profiler" "${profiles[0]}" --device "$device"
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
