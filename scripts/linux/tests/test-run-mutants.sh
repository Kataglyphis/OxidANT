#!/usr/bin/env bash
# run-mutants.sh against a fake cargo: the group's files, the report, the exit codes it passes, and the no-op without the tool.
set -u

# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
script="${tests_dir}/../run-mutants.sh"

mkdir -p "${work}/bin" "${work}/empty"
printf '#!/usr/bin/env bash\nexit 0\n' > "${work}/bin/cargo-mutants"
cat > "${work}/bin/cargo" <<'SH'
#!/usr/bin/env bash
printf 'ARG=%s\n' "$@"
out=""
while [ "$#" -gt 0 ]; do
    if [ "$1" = --output ]; then out="$2"; fi
    shift
done
mkdir -p "${out}/mutants.out"
printf 'a\nb\nc\n' > "${out}/mutants.out/caught.txt"
printf 'd\n' > "${out}/mutants.out/missed.txt"
exit "${FAKE_CARGO_RC:-0}"
SH
chmod +x "${work}/bin/cargo" "${work}/bin/cargo-mutants"
with_tool="${work}/bin:/usr/bin:/bin"
without_tool="${work}/empty:/usr/bin:/bin"

t_case "a group runs its files from docs/testing.md and reports the counts"
suite_run env PATH="${with_tool}" bash "${script}" capture "${work}/mut"
t_assert_eq 0 "$(cat "${work}/rc")"
t_assert_contains "$(cat "${work}/out")" "ARG=crates/observant/src/capture.rs"
t_assert_contains "$(cat "${work}/out")" "ARG=crates/observant/src/capture/logic.rs"
t_assert_contains "$(cat "${work}/out")" "ARG=--jobs"
t_assert_contains "$(cat "${work}/out")" "ARG=--lib"
t_assert_contains "$(cat "${work}/out")" "mutants capture caught: 3"
t_assert_contains "$(cat "${work}/out")" "mutants capture missed: 1"
t_assert_contains "$(cat "${work}/out")" "mutants capture timeout: 0"
t_assert_contains "$(cat "${work}/out")" $'ARG=-p\nARG=kataglyphis_observant'

t_case "the session group mutates the root package's slot, not ObservANT"
suite_run env PATH="${with_tool}" bash "${script}" session "${work}/mut"
t_assert_eq 0 "$(cat "${work}/rc")"
t_assert_contains "$(cat "${work}/out")" $'ARG=-p\nARG=oxidant\nARG=--file\nARG=src/session_slot.rs\n'
t_assert_no_grep "${work}/out" "crates/observant"
t_assert_contains "$(cat "${work}/out")" "mutants session missed: 1"

t_case "missed or timed-out mutants are a score, not a failed run"
suite_run env PATH="${with_tool}" FAKE_CARGO_RC=2 bash "${script}" network "${work}/mut"
t_assert_eq 0 "$(cat "${work}/rc")"
suite_run env PATH="${with_tool}" FAKE_CARGO_RC=3 bash "${script}" network "${work}/mut"
t_assert_eq 0 "$(cat "${work}/rc")"

t_case "a baseline that fails its tests fails the run"
suite_run env PATH="${with_tool}" FAKE_CARGO_RC=4 bash "${script}" media "${work}/mut"
t_assert_eq 4 "$(cat "${work}/rc")"

t_case "an unknown group is a usage error"
suite_run env PATH="${with_tool}" bash "${script}" everything "${work}/mut"
t_assert_eq 2 "$(cat "${work}/rc")"
t_assert_contains "$(cat "${work}/out")" "unknown group 'everything'"

t_case "without cargo-mutants in the image the job warns and runs nothing"
suite_run env PATH="${without_tool}" bash "${script}" arrival "${work}/mut"
t_assert_eq 0 "$(cat "${work}/rc")"
t_assert_contains "$(cat "${work}/out")" "::warning title=No cargo-mutants::"
t_assert_no_grep "${work}/out" "^ARG="
t_assert_grep "${work}/mut/arrival/mutants.out/not-run.txt" "hub CON238"

t_summary
