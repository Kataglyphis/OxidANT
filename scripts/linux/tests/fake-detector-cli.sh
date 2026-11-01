#!/usr/bin/env bash
# A stand-in for kataglyphis_cli onnx-detect: fixed two-cat results and a one-node profile per device.
set -euo pipefail
[[ $1 == onnx-detect ]] || exit 90
shift
tract=0 output='' profile=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    --tract) tract=1; shift ;;
    --image) shift 2 ;;
    --output) output=$2; shift 2 ;;
    --profile) profile=$2; shift 2 ;;
    *) exit 91 ;;
  esac
done
printf 'LAUNCHER %s\n' "${KATAGLYPHIS_ORT_DEVICE:-tract}" >> "${FAKE_SEEN:-/dev/null}"
score=0.9
if [[ $tract == 1 ]]; then
  [[ -z $profile ]] || exit 92
  [[ ${FAKE_TRACT_DRIFT:-0} == 0 ]] || score=0.6
else
  device=${KATAGLYPHIS_ORT_DEVICE:?}
  case "$device" in
    cpu) provider=CPUExecutionProvider ;;
    xnnpack) provider=XnnpackExecutionProvider ;;
    dnnl) provider=DnnlExecutionProvider ;;
    *) exit 93 ;;
  esac
  if [[ $device == dnnl && ${FAKE_DNNL:-run} != run ]]; then
    [[ $FAKE_DNNL == unavailable ]] && echo 'Error: ProviderUnavailable: DnnlExecutionProvider is absent' >&2
    exit 1
  fi
  [[ $device != xnnpack || ${FAKE_XNNPACK_ON_CPU:-0} == 0 ]] || provider=CPUExecutionProvider
  [[ $device != xnnpack || ${FAKE_XNNPACK_DRIFT:-0} == 0 ]] || score=0.6
  printf '[{"cat":"Node","args":{"provider":"%s","op_name":"Conv"}}]\n' "$provider" > "$profile.json"
fi
printf '{"schema_version":1,"width":100,"height":100,"detections":[{"class":15,"score":%s,"box":[10,60,30,95]},{"class":15,"score":0.7,"box":[50,20,80,60]}]}' "$score" > "$output"
