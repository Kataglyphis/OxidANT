#!/usr/bin/env bash
# Each format's parity check must be necessary to pass the package fixture.
set -euo pipefail
# shellcheck source=scripts/linux/tests/lib-fake-gpu.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-fake-gpu.sh"
repo="$fake_gpu_repo"
ANTFRASTRUCTURE_DIR=$(fake_gpu_hub_dir)
export ANTFRASTRUCTURE_DIR
fake_gpu_workspace
mkdir -p "$work/scripts/linux/tests" "$work/scripts/linux/lib" "$work/.github/workflows"
cp "${fake_gpu_tests_dir}/test-profile-gpu-packages.sh" "${fake_gpu_tests_dir}/lib-fake-gpu.sh" "$work/scripts/linux/tests/"
cp "${fake_gpu_tests_dir}/../lib/ort-runtime-bundle.sh" "${fake_gpu_tests_dir}/../lib/gpu-check-common.sh" "${fake_gpu_tests_dir}/../lib/antfrastructure.sh" \
  "${fake_gpu_tests_dir}/../lib/renovate-git-mode.sh" "$work/scripts/linux/lib/"
cp "${fake_gpu_tests_dir}/../profile-gpu-bundle.sh" "${fake_gpu_tests_dir}/../check-gpu-detector-parity.sh" "$work/scripts/linux/"
cp "${fake_gpu_tests_dir}/../lib/models-lock.sh" "$work/scripts/linux/lib/"
mkdir -p "$work/resources/models"
cp "${fake_gpu_tests_dir}/../../../resources/models/models.lock" "$work/resources/models/"
cp "${fake_gpu_tests_dir}/../../verify-ort-gpu-profile.py" "$work/scripts/"
cp "$repo/.github/workflows/linux-nvidia-gpu.yml" "$repo/.github/workflows/linux-x64-amd-gpu.yml" "$work/.github/workflows/"
profile="$work/scripts/linux/profile-gpu-packages.sh"
for format in tar deb appimage; do
  cp "${fake_gpu_tests_dir}/../profile-gpu-packages.sh" "$profile"
  # shellcheck disable=SC2016 # Match the literal variables in the profiled script.
  case "$format" in
    tar) pattern='${work}/tar/oxidant/oxidant' ;;
    deb) pattern='${work}/deb/opt/oxidant/oxidant' ;;
    appimage) pattern='${image}' ;;
  esac
  [[ $(grep -Fc 'check-gpu-detector-parity.sh" "'"$pattern" "$profile") -eq 1 ]] || { echo "Missing $format parity call" >&2; exit 1; }
  awk -v needle="$pattern" -v kind="$format" '
    index($0, "check-gpu-detector-parity.sh") && index($0, needle) {
      if (kind == "appimage") print "        true"
      next
    }
    { print }
  ' "$profile" > "$work/mutated"
  mv "$work/mutated" "$profile"
  fake_gpu_refuses "$format parity bypass survived the package fixture" "$work/$format.log" \
    bash "$work/scripts/linux/tests/test-profile-gpu-packages.sh"
  grep -Fq "$format detector mismatch was accepted" "$work/$format.log" || { cat "$work/$format.log" >&2; exit 1; }
done
echo 'Every package-format parity bypass was detected'
