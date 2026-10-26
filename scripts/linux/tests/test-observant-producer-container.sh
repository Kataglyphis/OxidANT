#!/usr/bin/env bash
set -u

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/linux/lib/antfrastructure.sh
source "${script_dir}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
# shellcheck source=scripts/linux/observant/lib/producer-container.sh
source "${script_dir}/../observant/lib/producer-container.sh"

work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

nerdctl() {
  printf '%s\0' "$@" > "${work}/args"
  return "${NERDCTL_TEST_RC:-0}"
}

logged_args() {
  local -a args=()
  mapfile -d '' -t args < "${work}/args"
  printf '<%q>' "${args[@]}"
}

expected_args() { printf '<%q>' "$@"; }

t_case 'image entrypoint probe uses the shared target volume and keeps its raw status'
t_assert_ok observant_producer_present family:latest target-vol /cargo-target/release/cat image
logged="$(logged_args)"
t_assert_contains "${logged}" "$(expected_args -v target-vol:/cargo-target family:latest bash -c)"
t_assert_contains "${logged}" "$(expected_args observant-probe /cargo-target/release/cat)"
NERDCTL_TEST_RC=7
t_assert_rc 7 observant_producer_present family:latest target-vol /cargo-target/release/cat image
NERDCTL_TEST_RC=0

t_case 'explicit bash entrypoint and platform stay before the image'
t_assert_ok observant_producer_present family:latest target-vol '/cargo-target/release/cat producer' bash --platform linux/arm64
logged="$(logged_args)"
t_assert_contains "${logged}" "$(expected_args run --platform linux/arm64)"
t_assert_contains "${logged}" "$(expected_args --entrypoint bash)"
t_assert_contains "${logged}" "$(expected_args observant-probe '/cargo-target/release/cat producer')"

t_case 'OxidANT build keeps its image entrypoint and workspace root'
t_assert_ok observant_build_producer family:latest '/host/OxidANT' target-vol cargo-vol /workspace image
logged="$(logged_args)"
t_assert_contains "${logged}" "$(expected_args --network host -v /host/OxidANT:/workspace)"
t_assert_contains "${logged}" "$(expected_args -v target-vol:/cargo-target -v cargo-vol:/cargo-home)"
t_assert_contains "${logged}" "$(expected_args -e CARGO_TARGET_DIR=/cargo-target -e CARGO_HOME=/cargo-home)"
t_assert_contains "${logged}" "$(expected_args family:latest bash -lc)"
t_assert_contains "${logged}" "$(expected_args observant-build /workspace)"
t_assert_contains "${logged}" kataglyphis_observant

t_case 'Omni build keeps its platform, entrypoint and nested source directory'
t_assert_ok observant_build_producer family:latest '/host/Omni AccelerANT' target-vol cargo-vol \
  /workspace/third_party/OxidANT bash --platform linux/arm64
logged="$(logged_args)"
t_assert_contains "${logged}" "$(expected_args run --platform linux/arm64)"
t_assert_contains "${logged}" "$(expected_args -v '/host/Omni AccelerANT:/workspace')"
t_assert_contains "${logged}" "$(expected_args --entrypoint bash family:latest -lc)"
t_assert_contains "${logged}" "$(expected_args observant-build /workspace/third_party/OxidANT)"
NERDCTL_TEST_RC=17
t_assert_rc 17 observant_build_producer family:latest /host/Omni target-vol cargo-vol \
  /workspace/third_party/OxidANT bash --platform linux/arm64
NERDCTL_TEST_RC=0

t_case 'unknown shell modes fail before a container starts'
rm -f "${work}/args"
t_assert_rc 2 observant_producer_present family:latest target-vol /cargo-target/release/cat invalid
t_assert_ok test ! -e "${work}/args"
t_assert_rc 2 observant_build_producer family:latest /host/Omni target-vol cargo-vol /workspace invalid
t_assert_ok test ! -e "${work}/args"

t_summary
