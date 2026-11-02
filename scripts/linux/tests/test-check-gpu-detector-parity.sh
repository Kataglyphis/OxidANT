#!/usr/bin/env bash
# Exercise the real CPU/GPU parity driver with a deterministic fake launcher.
set -euo pipefail
# shellcheck source=scripts/linux/tests/lib-fake-gpu.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-fake-gpu.sh"
hub="$fake_gpu_hub"
fake_gpu_workspace
mkdir -p "$work/runner"
export RUNNER_TEMP="$work/runner" TEST_FIXTURE="$work/two cats.jpg" TEST_SEEN="$work/seen"
printf fixture > "$TEST_FIXTURE"
fake_gpu_write_launcher "$work/launcher" <<'LAUNCHER'
fake_gpu_assert_unset LD_PRELOAD LD_AUDIT KATAGLYPHIS_ONNX_MODEL KATAGLYPHIS_ONNX_BACKEND ORT_DYLIB_PATH ORT_LIB_LOCATION \
  KATAGLYPHIS_PREPROCESS KATAGLYPHIS_SWAP_XY KATAGLYPHIS_ORT_THREADS KATAGLYPHIS_ORT_SPIN KATAGLYPHIS_SCORE_THRESHOLD
fake_gpu_assert_no_migraphx
[[ $1 == onnx-detect && $3 == "$TEST_FIXTURE" ]]
fake_gpu_detect_args "$@"
provider=$(fake_gpu_provider "$KATAGLYPHIS_ORT_DEVICE")
if [[ $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then
  [[ -z ${LD_LIBRARY_PATH+x} ]]
else
  [[ ${LD_LIBRARY_PATH:-} == /host/gpu-sdk ]]
fi
printf '%s\n' "$KATAGLYPHIS_ORT_DEVICE" >> "$TEST_SEEN"
if [[ ${FAKE_CPU_PROFILE:-0} == 1 ]]; then provider=CPUExecutionProvider; fi
if [[ ${FAKE_BAD_CPU:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then provider=CUDAExecutionProvider; fi
second_provider=$provider
if [[ ${FAKE_SECOND_CPU:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]]; then second_provider=CPUExecutionProvider; fi
if [[ ${FAKE_CPU_SECOND_WRONG:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then second_provider=CUDAExecutionProvider; fi
fake_gpu_two_run_profile "$7.json" "$provider" "$second_provider"
score=0.9
if [[ ${FAKE_LOW_GPU:-0} == 1 ]]; then
  if [[ $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then score=0.81; else score=0.78; fi
fi
score=$(fake_gpu_drift_score "${FAKE_MISMATCH:-0}" 1 "$score")
fake_gpu_detections "$5" "$score"
if [[ $KATAGLYPHIS_ORT_DEVICE == cpu && ${FAKE_CPU_NO_REPEAT:-0} != 1 ]] || [[ $KATAGLYPHIS_ORT_DEVICE != cpu && ${FAKE_NO_REPEAT:-0} != 1 ]]; then
  if [[ ${FAKE_SECOND_MISMATCH:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]] || [[ ${FAKE_CPU_SECOND_MISMATCH:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE == cpu ]]; then score=0.6; fi
  if [[ ${FAKE_LOW_GPU:-0} == 1 && $KATAGLYPHIS_ORT_DEVICE != cpu ]]; then score=0.77; fi
  fake_gpu_detections "$9" "$score"
fi
LAUNCHER
fake_gpu_poison_env
fake_gpu_poison_migraphx
export KATAGLYPHIS_PREPROCESS=letterbox KATAGLYPHIS_SWAP_XY=1 KATAGLYPHIS_ORT_THREADS=1 KATAGLYPHIS_ORT_SPIN=0
export KATAGLYPHIS_SCORE_THRESHOLD=0.99
for device in cuda migraphx; do
  : > "$TEST_SEEN"
  bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null
  printf 'cpu\n%s\n' "$device" > "$work/expected"
  cmp "$work/expected" "$TEST_SEEN"
  FAKE_LOW_GPU=1 bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE" >/dev/null
  fake_gpu_refuses 'Different GPU detections were accepted' /dev/null env FAKE_MISMATCH=1 bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE"
  fake_gpu_refuses 'CPU-only profile was accepted as GPU execution' /dev/null env FAKE_CPU_PROFILE=1 bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE"
  for flag in FAKE_SECOND_MISMATCH FAKE_SECOND_CPU FAKE_NO_REPEAT; do
    fake_gpu_refuses "Repeated GPU inference failure was accepted: $flag" /dev/null env "$flag=1" bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE"
  done
  for flag in FAKE_CPU_SECOND_MISMATCH FAKE_CPU_SECOND_WRONG FAKE_CPU_NO_REPEAT; do
    fake_gpu_refuses "Repeated CPU inference failure was accepted: $flag" /dev/null env "$flag=1" bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" "$device" "$TEST_FIXTURE"
  done
done
fake_gpu_refuses 'Missing real fixture was accepted' /dev/null bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" cuda "$work/missing.jpg"
mkdir -p "$work/mutant/scripts/linux/lib"
cp "${fake_gpu_tests_dir}/../lib/gpu-check-common.sh" "$work/mutant/scripts/linux/lib/"
cp "${fake_gpu_tests_dir}/../../verify-ort-gpu-profile.py" "$work/mutant/scripts/"
# shellcheck disable=SC2016
sed 's@"$work/$device-repeat.json"@"$work/cpu.json"@g; s@"$work/$device.json"@"$work/cpu.json"@g' "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" > "$work/mutant/scripts/linux/check.sh"
fake_gpu_accepts 'The CPU-versus-CPU comparison mutant was not exercised' env ANTFRASTRUCTURE_DIR="$hub" FAKE_MISMATCH=1 bash "$work/mutant/scripts/linux/check.sh" "$work/launcher" cuda "$TEST_FIXTURE"
# shellcheck disable=SC2016
sed 's@^  python3 -I "$intervals".*@  :@' "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" > "$work/mutant/scripts/linux/check.sh"
for flag in FAKE_BAD_CPU FAKE_SECOND_CPU; do
  fake_gpu_accepts "The missing per-run provider check mutant was not exercised: $flag" env "$flag=1" ANTFRASTRUCTURE_DIR="$hub" bash "$work/mutant/scripts/linux/check.sh" "$work/launcher" cuda "$TEST_FIXTURE"
done
# shellcheck disable=SC2016
sed 's@^python3 -I "$comparator" "$work/cpu.json" "$work/$device-repeat.json"@:@; s@^python3 -I "$comparator" --pairwise "$work/cpu-repeat.json" "$work/$device-repeat.json"@:@; s@^python3 -I "$comparator" --pairwise "$work/$device.json" "$work/$device-repeat.json"@:@' "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" > "$work/mutant/scripts/linux/check.sh"
fake_gpu_accepts 'The missing repeated-output comparisons mutant was not exercised' env ANTFRASTRUCTURE_DIR="$hub" FAKE_SECOND_MISMATCH=1 bash "$work/mutant/scripts/linux/check.sh" "$work/launcher" cuda "$TEST_FIXTURE"
mkdir -p "$work/missing-hub"
fake_gpu_hub_refuses "$work/missing-hub" 'lacks detector parity comparison' 'Missing shared parity comparator was accepted' "$work/missing.log" bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" cuda "$TEST_FIXTURE"
mkdir -p "$work/missing-hub/shared/python"
cp "$hub/shared/python/compare_detector_parity.py" "$hub/shared/python/verify_ort_profile.py" "$work/missing-hub/shared/python/"
fake_gpu_hub_refuses "$work/missing-hub" 'lacks GPU profile environment isolation' 'Missing GPU profile environment helper was accepted' "$work/missing.log" bash "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/launcher" cuda "$TEST_FIXTURE"
echo 'Detector parity driver checks passed'
