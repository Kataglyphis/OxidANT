#!/usr/bin/env bash
# Each format's parity check must be necessary to pass the package fixture.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../../.." && pwd)
export ANTFRASTRUCTURE_DIR="${ANTFRASTRUCTURE_DIR:-$repo/third_party/ANTfrastructure}"
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/scripts/linux/tests" "$work/scripts/linux/lib" "$work/.github/workflows"
cp "$here/test-profile-gpu-packages.sh" "$work/scripts/linux/tests/"
cp "$here/../lib/ort-runtime-bundle.sh" "$here/../lib/antfrastructure.sh" \
  "$here/../lib/renovate-git-mode.sh" "$work/scripts/linux/lib/"
cp "$here/../profile-gpu-bundle.sh" "$here/../check-gpu-detector-parity.sh" "$work/scripts/linux/"
cp "$here/../lib/models-lock.sh" "$work/scripts/linux/lib/"
mkdir -p "$work/resources/models"
cp "$here/../../../resources/models/models.lock" "$work/resources/models/"
cp "$here/../../verify-ort-gpu-profile.py" "$work/scripts/"
cp "$repo/.github/workflows/linux-nvidia-gpu.yml" "$repo/.github/workflows/linux-x64-amd-gpu.yml" "$work/.github/workflows/"
profile="$work/scripts/linux/profile-gpu-packages.sh"
for format in tar deb appimage; do
  cp "$here/../profile-gpu-packages.sh" "$profile"
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
  if bash "$work/scripts/linux/tests/test-profile-gpu-packages.sh" >"$work/$format.log" 2>&1; then
    echo "$format parity bypass survived the package fixture" >&2; exit 1
  fi
  grep -Fq "$format detector mismatch was accepted" "$work/$format.log" || { cat "$work/$format.log" >&2; exit 1; }
done
echo 'Every package-format parity bypass was detected'
