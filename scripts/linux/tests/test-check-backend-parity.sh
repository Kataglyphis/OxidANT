#!/usr/bin/env bash
# The backend parity driver against a fake CLI: each drift, CPU fallback and unnamed DNNL failure must fail it.
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=../lib/antfrastructure.sh
source "$here/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
printf fixture > "$work/photo.jpg"
cp "$here/fake-detector-cli.sh" "$work/cli"
chmod +x "$work/cli"
export FAKE_SEEN="$work/seen" RUNNER_TEMP="$work"

check() {
  local rc=0
  : > "$FAKE_SEEN"
  env "$@" bash "$here/../check-backend-parity.sh" "$work/cli" "$work/photo.jpg" > "$work/out" 2>&1 || rc=$?
  printf '%s\n' "$rc" > "$work/rc"
}

check FAKE_DNNL=run
t_case "every backend agreeing passes and runs cpu, tract, xnnpack and dnnl"
t_assert_grep "$work/rc" '^0$'
t_assert_grep "$work/out" '^ORT and tract agree$'
t_assert_grep "$work/out" '^ORT CPU and DNNL agree$'
for device in cpu tract xnnpack dnnl; do t_assert_grep "$FAKE_SEEN" "^LAUNCHER ${device}$"; done

check FAKE_DNNL=unavailable
t_case "DNNL reported as ProviderUnavailable passes"
t_assert_grep "$work/rc" '^0$'
t_assert_grep "$work/out" '^DNNL is ProviderUnavailable here$'

check FAKE_DNNL=crash
t_case "a DNNL failure without ProviderUnavailable fails"
t_assert_grep "$work/rc" '^1$'
t_assert_grep "$work/out" 'DNNL failed without ProviderUnavailable'

check FAKE_TRACT_DRIFT=1
t_case "tract drifting from ORT fails"
t_assert_grep "$work/rc" '^1$'
t_assert_no_grep "$work/out" '^ORT and tract agree$'

check FAKE_XNNPACK_ON_CPU=1
t_case "an XNNPACK request that ran on the CPU provider fails"
t_assert_grep "$work/rc" '^1$'
t_assert_grep "$work/out" 'No model operators executed on XnnpackExecutionProvider'

check FAKE_XNNPACK_DRIFT=1
t_case "XNNPACK drifting from CPU fails"
t_assert_grep "$work/rc" '^1$'
t_assert_no_grep "$work/out" '^ORT CPU and XNNPACK agree$'
t_summary
