#!/usr/bin/env bash
set -u

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/linux/lib/antfrastructure.sh
source "${script_dir}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
t_skip_unless 'Python 3.11+ with tomllib' python3 -c 'import tomllib'
root="$(cd -- "${script_dir}/../../.." && pwd)"
gate="${script_dir}/../check-burn-lock-graph.py"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

t_case 'the current lockfile has no old Torch or Zstd chain'
t_assert_rc 0 python3 "${gate}" "${root}/Cargo.lock"

for entry in 'burn-tch 0.22.0' 'tch 0.22.0' 'torch-sys 0.22.0' \
  'zip 0.6.6' 'zstd 0.11.2' 'zstd-safe 5.0.2' 'zstd-sys 2.1.1'; do
  name="${entry% *}"
  value="${entry#* }"
  cp "${root}/Cargo.lock" "${work}/Cargo.lock"
  printf '\n[[package]]\nname = "%s"\nversion = "%s"\n' "${name}" "${value}" >> "${work}/Cargo.lock"
  t_case "reintroducing ${name} ${value} fails"
  t_assert_rc 1 python3 "${gate}" "${work}/Cargo.lock"
done

t_case 'a malformed lockfile cannot pass'
printf '[[package]]\nname = "broken"\nversion = ' > "${work}/Cargo.lock"
t_assert_rc 2 python3 "${gate}" "${work}/Cargo.lock"

t_summary
