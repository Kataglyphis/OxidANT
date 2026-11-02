#!/usr/bin/env bash
# Sourced first by a harness suite: publishes tests_dir and work (antfrastructure.sh publishes KATAGLYPHIS_REPO_ROOT), loads the hub harness and removes work on exit.
tests_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/linux/lib/antfrastructure.sh
source "${tests_dir}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
work="$(mktemp -d)"
trap 'rm -rf -- "${work}"' EXIT

# suite_run COMMAND...: its combined output in ${work}/out, its status in ${work}/rc.
suite_run() {
    local rc=0
    "$@" > "${work}/out" 2>&1 || rc=$?
    printf '%s\n' "${rc}" > "${work}/rc"
}
