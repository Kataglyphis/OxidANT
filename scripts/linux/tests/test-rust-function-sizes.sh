#!/usr/bin/env bash
# check-rust-function-sizes.sh on fixture trees: a frozen function may shrink, not grow, and a new long one fails; not covered: clippy's own count.
set -u
tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${tests_dir}/../../.." && pwd)"
source "${repo}/scripts/linux/lib/antfrastructure.sh"
# shellcheck source=third_party/ANTfrastructure/linux/scripts/tests/test-harness.sh
source "$(antfrastructure_path linux/scripts/tests/test-harness.sh)"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
hub="$(antfrastructure_path .)"
check="${repo}/scripts/linux/check-rust-function-sizes.sh"

# fixture <dir> <lines of long_one> [lines of second]: a git tree whose src/lib.rs holds functions of those sizes.
fixture() {
  local dir="$1" first="$2" second="${3:-0}" n
  mkdir -p "${dir}/src"
  {
    printf 'fn long_one() {\n'
    for ((n = 2; n < first; n++)); do printf '    let _ = %d;\n' "${n}"; done
    printf '}\n'
    if [ "${second}" -gt 0 ]; then
      printf 'fn second() {\n'
      for ((n = 2; n < second; n++)); do printf '    let _ = %d;\n' "${n}"; done
      printf '}\n'
    fi
  } > "${dir}/src/lib.rs"
  printf 'src/lib.rs | long_one | 90 | a fixture\n' > "${dir}/rust-function-size.allow"
  git -C "${dir}" init -q || return
  git -C "${dir}" add -A
}

run() { bash "${check}" --root "$1" --hub "${hub}" > "${work}/out" 2>&1; printf '%s' "$?"; }

fixture "${work}/same" 90
t_case 'a frozen function at its budget passes'
t_assert_eq 0 "$(run "${work}/same")" 'exit status'
t_assert_grep "${work}/out" '^rust function sizes: OK$'

fixture "${work}/grown" 95
t_case 'a frozen function that grew fails and says by how much'
t_assert_eq 1 "$(run "${work}/grown")" 'exit status'
t_assert_grep "${work}/out" '^GREW: src/lib.rs long_one from 90 to 95 lines'

fixture "${work}/new" 90 85
t_case 'a new function over 80 lines fails, the frozen one beside it does not'
t_assert_eq 1 "$(run "${work}/new")" 'exit status'
t_assert_grep "${work}/out" '^NEW: src/lib.rs second has 85 lines'
t_assert_no_grep "${work}/out" 'GREW'

fixture "${work}/shrunk" 70
t_case 'a frozen function that is short now passes and asks for its row to go'
t_assert_eq 0 "$(run "${work}/shrunk")" 'exit status'
t_assert_grep "${work}/out" 'long_one is 80 lines or fewer now; drop its row'

t_summary
