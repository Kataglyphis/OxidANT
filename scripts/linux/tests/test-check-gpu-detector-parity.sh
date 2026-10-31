#!/usr/bin/env bash
# Exercise the real CPU/GPU parity driver with a deterministic fake launcher.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../../.." && pwd)
hub="${ANTFRASTRUCTURE_DIR:-$repo/third_party/ANTfrastructure}"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/runner"
export RUNNER_TEMP="$work/runner" TEST_FIXTURE="$work/two cats.jpg" TEST_SEEN="$work/seen"
printf fixture > "$TEST_FIXTURE"
cat > "$work/launcher" <<'LAUNCHER'
#!/usr/bin/env bash
set -euo pipefail
[[ -z ${LD_PRELOAD+x}${LD_AUDIT+x}${KATAGLYPHIS_ONNX_MODEL+x}${KATAGLYPHIS_ONNX_BACKEND+x}${ORT_DYLIB_PATH+x}${ORT_LIB_LOCATION+x}${KATAGLYPHIS_PREPROCESS+x}${KATAGLYPHIS_SWAP_XY+x}${KATAGLYPHIS_ORT_THREADS+x}${KATAGLYPHIS_ORT_SPIN+x}${KATAGLYPHIS_SCORE_THRESHOLD+x} ]]
[[ -z ${ORT_MIGRAPHX_FP16_ENABLE+x}${ORT_MIGRAPHX_MODEL_CACHE_PATH+x}${MIGRAPHX_SKIP_BENCHMARKING+x}${MIGRAPHX_UNKNOWN_OVERRIDE+x} ]]
[[ $1 == onnx-detect && $2 == --image && $3 == "$TEST_FIXTURE" && $4 == --output && $6 == --profile ]]
case "$KATAGLYPHIS_ORT_DEVICE" in
  cpu) [[ $# -eq 9 && $8 == --repeat-output && -z ${LD_LIBRARY_PATH+x} ]]; provider=CPUExecutionProvider ;;
  cuda) [[ $# -eq 9 && $8 == --repeat-output && ${LD_LIBRARY_PATH:-} == /host/gpu-sdk ]]; provider=CUDAExecutionProvider ;;
  migraphx) [[ $# -eq 9 && $8 == --repeat-output && ${LD_LIBRARY_PATH:-} == /host/gpu-sdk ]]; provider=MIGraphXExecutionProvider ;;
  *) exit 1 ;;
esac
printf '%s\n' "$KATAGLYPHIS_ORT_DEVICE" >> "$TEST_SEEN"
if [[ ${FAKE_CPU_PROFILE:-0} == 1 ]]; then provider=CPUExecutionProvider; fi
if [[ ${FAKE_BAD_CPU:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then provider=CUDAExecutionProvider; fi
second_provider=$provider
if [[ ${FAKE_SECOND_CPU:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]]; then second_provider=CPUExecutionProvider; fi
if [[ ${FAKE_CPU_SECOND_WRONG:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then second_provider=CUDAExecutionProvider; fi
printf '[{"cat":"Session","name":"model_run","ph":"X","pid":1,"ts":0,"dur":90},{"cat":"Node","ph":"X","pid":1,"ts":10,"dur":20,"args":{"provider":"%s","op_name":"Conv"}}' "$provider" > "$7.json"
printf ',{"cat":"Session","name":"model_run","ph":"X","pid":1,"ts":100,"dur":90},{"cat":"Node","ph":"X","pid":1,"ts":110,"dur":20,"args":{"provider":"%s","op_name":"Conv"}}' "$second_provider" >> "$7.json"
printf ']\n' >> "$7.json"
score=0.9
if [[ ${FAKE_LOW_GPU:-0} == 1 ]]; then
  if [[ $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then score=0.81; else score=0.78; fi
fi
if [[ ${FAKE_MISMATCH:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]]; then score=0.6; fi
printf '{"schema_version":1,"width":100,"height":100,"detections":[{"class":15,"score":%s,"box":[10,60,30,95]},{"class":15,"score":0.7,"box":[50,20,80,60]}]}' "$score" > "$5"
if [[ $KATAGLYPHIS_ORT_DEVICE == cpu && ${FAKE_CPU_NO_REPEAT:-0} != 1 ]] || [[ $KATAGLYPHIS_ORT_DEVICE != cpu && ${FAKE_NO_REPEAT:-0} != 1 ]]; then
  if [[ ${FAKE_SECOND_MISMATCH:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]] || [[ ${FAKE_CPU_SECOND_MISMATCH:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then score=0.6; fi
  if [[ ${FAKE_LOW_GPU:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]]; then score=0.77; fi
  printf '{"schema_version":1,"width":100,"height":100,"detections":[{"class":15,"score":%s,"box":[10,60,30,95]},{"class":15,"score":0.7,"box":[50,20,80,60]}]}' "$score" > "$9"
fi
LAUNCHER
chmod +x "$work/launcher"
export KATAGLYPHIS_ONNX_MODEL=/wrong KATAGLYPHIS_ONNX_BACKEND=tract ORT_DYLIB_PATH=/wrong ORT_LIB_LOCATION=/wrong LD_LIBRARY_PATH=/host/gpu-sdk
export KATAGLYPHIS_PREPROCESS=letterbox KATAGLYPHIS_SWAP_XY=1 KATAGLYPHIS_ORT_THREADS=1 KATAGLYPHIS_ORT_SPIN=0
export KATAGLYPHIS_SCORE_THRESHOLD=0.99
export ORT_MIGRAPHX_FP16_ENABLE=1 ORT_MIGRAPHX_MODEL_CACHE_PATH=/wrong MIGRAPHX_SKIP_BENCHMARKING=1 MIGRAPHX_UNKNOWN_OVERRIDE=poison
export LD_PRELOAD='' LD_AUDIT=''
for device in cuda migraphx; do
  : > "$TEST_SEEN"
  bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null
  printf 'cpu\n%s\n' "$device" > "$work/expected"
  cmp "$work/expected" "$TEST_SEEN"
  FAKE_LOW_GPU=1 bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null
  if FAKE_MISMATCH=1 bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null 2>&1; then
    echo 'Different GPU detections were accepted' >&2; exit 1
  fi
  if FAKE_CPU_PROFILE=1 bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null 2>&1; then
    echo 'CPU-only profile was accepted as GPU execution' >&2; exit 1
  fi
  for flag in FAKE_SECOND_MISMATCH FAKE_SECOND_CPU FAKE_NO_REPEAT; do
    if env "$flag=1" bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null 2>&1; then
      echo "Repeated GPU inference failure was accepted: $flag" >&2; exit 1
    fi
  done
  for flag in FAKE_CPU_SECOND_MISMATCH FAKE_CPU_SECOND_WRONG FAKE_CPU_NO_REPEAT; do
    if env "$flag=1" bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null 2>&1; then
      echo "Repeated CPU inference failure was accepted: $flag" >&2; exit 1
    fi
  done
done
if bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" cuda "$work/missing.jpg" >/dev/null 2>&1; then
  echo 'Missing real fixture was accepted' >&2; exit 1
fi
mkdir -p "$work/mutant/scripts/linux"
cp "$here/../../verify-ort-gpu-profile.py" "$work/mutant/scripts/"
# shellcheck disable=SC2016
sed 's@"$work/$device-repeat.json"@"$work/cpu.json"@g; s@"$work/$device.json"@"$work/cpu.json"@g' "$here/../check-gpu-detector-parity.sh" > "$work/mutant/scripts/linux/check.sh"
if ! ANTFRASTRUCTURE_DIR="$hub" FAKE_MISMATCH=1 bash "$work/mutant/scripts/linux/check.sh" "$work/launcher" cuda "$TEST_FIXTURE" >/dev/null 2>&1; then
  echo 'The CPU-versus-CPU comparison mutant was not exercised' >&2; exit 1
fi
# shellcheck disable=SC2016
sed 's@^  python3 -I "$intervals".*@  :@' "$here/../check-gpu-detector-parity.sh" > "$work/mutant/scripts/linux/check.sh"
for flag in FAKE_BAD_CPU FAKE_SECOND_CPU; do
  if ! env "$flag=1" ANTFRASTRUCTURE_DIR="$hub" bash "$work/mutant/scripts/linux/check.sh" "$work/launcher" cuda "$TEST_FIXTURE" >/dev/null 2>&1; then
    echo "The missing per-run provider check mutant was not exercised: $flag" >&2; exit 1
  fi
done
# shellcheck disable=SC2016
sed 's@^python3 -I "$comparator" "$work/cpu.json" "$work/$device-repeat.json"@:@; s@^python3 -I "$comparator" --pairwise "$work/cpu-repeat.json" "$work/$device-repeat.json"@:@; s@^python3 -I "$comparator" --pairwise "$work/$device.json" "$work/$device-repeat.json"@:@' "$here/../check-gpu-detector-parity.sh" > "$work/mutant/scripts/linux/check.sh"
if ! ANTFRASTRUCTURE_DIR="$hub" FAKE_SECOND_MISMATCH=1 bash "$work/mutant/scripts/linux/check.sh" "$work/launcher" cuda "$TEST_FIXTURE" >/dev/null 2>&1; then
  echo 'The missing repeated-output comparisons mutant was not exercised' >&2; exit 1
fi
mkdir -p "$work/missing-hub"
if ANTFRASTRUCTURE_DIR="$work/missing-hub" bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" cuda "$TEST_FIXTURE" >"$work/missing.log" 2>&1; then
  echo 'Missing shared parity comparator was accepted' >&2; exit 1
fi
grep -q 'lacks detector parity comparison' "$work/missing.log"
mkdir -p "$work/missing-hub/shared/python"
cp "$hub/shared/python/compare_detector_parity.py" "$hub/shared/python/verify_ort_profile.py" "$work/missing-hub/shared/python/"
if ANTFRASTRUCTURE_DIR="$work/missing-hub" bash "$here/../check-gpu-detector-parity.sh" "$work/launcher" cuda "$TEST_FIXTURE" >"$work/missing.log" 2>&1; then
  echo 'Missing GPU profile environment helper was accepted' >&2; exit 1
fi
grep -q 'lacks GPU profile environment isolation' "$work/missing.log"
echo 'Detector parity driver checks passed'
