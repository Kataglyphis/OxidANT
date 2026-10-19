#!/usr/bin/env bash
# The benchmark report on synthetic Criterion output: the median of earlier runs, its warning and its failure.
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/antfrastructure.sh
source "${SCRIPT_DIR}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
report="${SCRIPT_DIR}/../bench-report.py"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

# check <name> <file> <grep -E pattern> [absent]: the file holds the pattern, or with "absent" does not.
check() {
  t_case "$1"
  if [ "${4:-present}" = absent ]; then t_assert_no_grep "$2" "$3"; else t_assert_grep "$2" "$3"; fi
}

# criterion <dir> <detector ns> <draw_box ns>: Criterion's layout for two benches.
criterion() {
  mkdir -p "$1/detector/new" "$1/frame/draw_box/new"
  printf '{"mean":{"point_estimate":%s}}\n' "$2" > "$1/detector/new/estimates.json"
  printf '{"mean":{"point_estimate":%s}}\n' "$3" > "$1/frame/draw_box/new/estimates.json"
}

# report <args...> -> rc, stdout and stderr in ${work}.
report() {
  local rc=0
  python3 "$report" "$@" > "${work}/out" 2> "${work}/err" || rc=$?
  printf '%s\n' "$rc" > "${work}/rc"
}

mkdir -p "${work}/history" "${work}/two"
printf '{"detector": 50000000, "frame/draw_box": 1000}\n' > "${work}/history/1.json"
printf '{"detector": 54000000, "frame/draw_box": 1100}\n' > "${work}/history/2.json"
printf '{"detector": 52000000, "frame/draw_box": 900}\n' > "${work}/history/3.json"
cp "${work}/history/1.json" "${work}/history/2.json" "${work}/two/"
criterion "${work}/slower" 70000000 1000
criterion "${work}/far" 110000000 1000
criterion "${work}/tiny-far" 52000000 5000
criterion "${work}/same" 53000000 1000

report "${work}/slower" "${work}/history" --write "${work}/means.json"
check "the table shows the mean and the change against the median" "${work}/out" '^\| `detector` \| 70\.00 ms \| \+34\.6 % \(median 52\.00 ms of 3\) \|$'
check "a slowdown over the threshold warns" "${work}/err" '^::warning title=Benchmark slower::detector \+34\.6 % against the median of 3 runs$'
check "a warning alone passes" "${work}/rc" '^0$'
check "--write keeps this run's means" "${work}/means.json" '"detector": 70000000'

report "${work}/far" "${work}/history"
check "a doubling fails" "${work}/rc" '^1$'
check "and says so as an error" "${work}/err" '^::error title=Benchmark far slower::detector \+111\.5 %'

report "${work}/far" "${work}/two"
check "two earlier runs are too few to fail" "${work}/rc" '^0$'

report "${work}/tiny-far" "${work}/history"
check "a microsecond bench five times slower only warns" "${work}/rc" '^0$'
check "but it does warn" "${work}/err" '^::warning title=Benchmark slower::frame/draw_box \+400\.0 %'

report "${work}/same" "${work}/history"
check "within the threshold, no annotation" "${work}/err" '::' absent

report "${work}/same" "${work}/missing"
check "no history yet says so" "${work}/out" '^\| `detector` \| 53\.00 ms \| no history yet \|$'

report "${work}/nothing" "${work}/history"
check "no results say so" "${work}/out" '^No benchmark results under'

t_summary
