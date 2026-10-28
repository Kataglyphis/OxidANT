#!/usr/bin/env bash
set -u

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/linux/lib/antfrastructure.sh
source "${script_dir}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
t_skip_unless 'Python 3.11+ with tomllib' python3 -c 'import tomllib'
root="$(cd -- "${script_dir}/../../.." && pwd)"
gate="${script_dir}/../check-cargo-patches-used.py"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

t_case 'every patched source copy in this workspace is in its lockfile'
t_assert_rc 0 python3 "${gate}" "${root}"

mkdir -p "${work}/vendor/demo-1.0.0"
printf '[package]\nname = "demo"\nversion = "1.0.0"\n' > "${work}/vendor/demo-1.0.0/Cargo.toml"
printf '[workspace]\n\n[patch.crates-io]\ndemo = { path = "vendor/demo-1.0.0" }\n' > "${work}/Cargo.toml"

t_case 'a patched copy that the lockfile holds as a path package passes'
printf '[[package]]\nname = "demo"\nversion = "1.0.0"\n' > "${work}/Cargo.lock"
t_assert_rc 0 python3 "${gate}" "${work}"

t_case 'lock maintenance that moved past the patched version fails'
printf '[[package]]\nname = "demo"\nversion = "1.0.1"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n' > "${work}/Cargo.lock"
t_assert_rc 1 python3 "${gate}" "${work}"
t_assert_contains "$(python3 "${gate}" "${work}" 2>&1)" 'vendor/demo-1.0.0 (demo 1.0.0) is not in Cargo.lock'

t_case 'the same version from the registry is not the patched copy'
printf '[[package]]\nname = "demo"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n' > "${work}/Cargo.lock"
t_assert_rc 1 python3 "${gate}" "${work}"

t_case 'a malformed lockfile cannot pass'
printf '[[package]]\nname = "demo"\nversion = ' > "${work}/Cargo.lock"
t_assert_rc 2 python3 "${gate}" "${work}"

t_summary
