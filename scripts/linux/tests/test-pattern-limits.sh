#!/usr/bin/env bash
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
t_skip_unless 'Python 3.11+ with tomllib' python3 -c 'import tomllib'
gate="${KATAGLYPHIS_REPO_ROOT}/third_party/ANTfrastructure/linux/scripts/verify_pattern_limits.py"

# The workspace itself is graded by run-lint-gates.sh; this proves the row on a planted tree.
cp "${KATAGLYPHIS_REPO_ROOT}/quality-report.toml" "${work}/"
mkdir -p "${work}/crates/probe" "${work}/third_party/vendored"
printf '[package]\nname = "probe"\n\n[features]\ndefault = []\n' > "${work}/crates/probe/Cargo.toml"
printf '[features]\nstd = []\n' > "${work}/third_party/vendored/Cargo.toml"
git -C "${work}" init -q && git -C "${work}" add -A

t_case 'a planted [features] table fails the lint gates and is named'
t_assert_rc 1 python3 "${gate}" --root "${work}"
t_assert_contains "$(python3 "${gate}" --root "${work}" 2>&1)" 'crates/probe/Cargo.toml:4:'

t_case 'a vendored crate keeps its upstream features'
git -C "${work}" rm -q --cached crates/probe/Cargo.toml
t_assert_rc 0 python3 "${gate}" --root "${work}"

t_summary
