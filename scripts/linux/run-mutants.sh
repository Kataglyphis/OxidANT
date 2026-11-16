#!/usr/bin/env bash
# One group of ObservANT's cargo-mutants commands from docs/testing.md § Coverage and mutation testing, for the weekly job: it reports scores and gates nothing.
set -euo pipefail

usage() {
    echo "usage: run-mutants.sh <network|arrival|camera|media|capture> [output dir, default logs/mutants]" >&2
}

group="${1-}"
out="${2:-logs/mutants}"
case "${group}" in
    network) files=(web.rs static_files.rs) ;;
    arrival) files=(motion.rs presence.rs status.rs) ;;
    camera) files=(camera.rs config.rs notify.rs) ;;
    media) files=(clip.rs frame.rs stats.rs output.rs precompress.rs) ;;
    capture) files=(capture.rs capture/logic.rs) ;;
    *)
        echo "run-mutants.sh: unknown group '${group}'" >&2
        usage
        exit 2
        ;;
esac

results="${out}/${group}/mutants.out"

# The image pins no cargo-mutants yet (hub CON238): the job leaves a note to upload instead of a score.
if ! command -v cargo-mutants > /dev/null 2>&1; then
    mkdir -p "${results}"
    echo "not run: the image has no cargo-mutants (hub CON238)" > "${results}/not-run.txt"
    echo "::warning title=No cargo-mutants::the image has no cargo-mutants (hub CON238); group ${group} did not run"
    exit 0
fi

file_args=()
for file in "${files[@]}"; do file_args+=(--file "crates/observant/src/${file}"); done
rc=0
cargo mutants -p kataglyphis_observant "${file_args[@]}" --jobs 1 --output "${out}/${group}" -- --lib || rc=$?
# 2 is missed mutants and 3 timed-out ones: the score this job reports, not a broken run.
case "${rc}" in
    0 | 2 | 3) ;;
    *) exit "${rc}" ;;
esac

for kind in caught missed timeout unviable; do
    count=0
    [ -f "${results}/${kind}.txt" ] && count="$(wc -l < "${results}/${kind}.txt")"
    printf 'mutants %s %s: %s\n' "${group}" "${kind}" "${count}"
done
