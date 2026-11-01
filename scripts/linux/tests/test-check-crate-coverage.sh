#!/usr/bin/env bash
# The per-crate coverage floor on a synthetic cobertura.xml: each crate graded alone, and every way it must go red.
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/antfrastructure.sh
source "${SCRIPT_DIR}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
gate="${SCRIPT_DIR}/../check-crate-coverage.py"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

# A well-covered crate (9 of 10 lines), a poorly covered root crate (1 of 4), and a vendored file outside both.
cat > "${work}/cobertura.xml" <<'XML'
<?xml version="1.0"?>
<coverage><packages><package name="p"><classes>
  <class name="a" filename="crates/observant/src/web.rs"><lines>
    <line number="1" hits="3"/><line number="2" hits="1"/><line number="3" hits="1"/><line number="4" hits="1"/><line number="5" hits="1"/>
  </lines></class>
  <class name="b" filename="crates/observant/src/clip.rs"><lines>
    <line number="1" hits="1"/><line number="2" hits="1"/><line number="3" hits="1"/><line number="4" hits="1"/><line number="5" hits="0"/>
  </lines></class>
  <class name="c" filename="src/webcam_engine.rs"><lines>
    <line number="1" hits="1"/><line number="2" hits="0"/><line number="3" hits="0"/><line number="4" hits="0"/>
  </lines></class>
  <class name="d" filename="/usr/local/cargo/registry/src/x/lib.rs"><lines>
    <line number="1" hits="0"/>
  </lines></class>
</classes></package></packages></coverage>
XML

# grade <rc> <name> <args...>: the gate's exit code for these arguments.
grade() {
  t_case "$2"
  t_assert_rc "$1" python3 "$gate" "${@:3}"
}

grade 0 "both crates over their floors" "${work}/cobertura.xml" --floor crates/observant=90 --floor src=25
grade 1 "the root crate under its floor" "${work}/cobertura.xml" --floor crates/observant=90 --floor src=26
grade 1 "the good crate does not carry the bad one" "${work}/cobertura.xml" --floor crates/observant=50 --floor src=50
grade 1 "a floor for a crate with no lines" "${work}/cobertura.xml" --floor crates/observant=90 --floor src=25 --floor crates/telemetry=1
grade 1 "a crate without a floor" "${work}/cobertura.xml" --floor src=25
grade 0 "a large slack warns without failing" "${work}/cobertura.xml" --floor crates/observant=86 --floor src=23 --max-slack 3
grade 2 "no floor at all" "${work}/cobertura.xml"
grade 2 "a malformed floor" "${work}/cobertura.xml" --floor src=half

python3 "$gate" "${work}/cobertura.xml" --floor crates/observant=90 --floor src=0 > "${work}/out" 2>&1
t_case "the table counts lines per crate"
t_assert_grep "${work}/out" '^crates/observant +9 / +10 +90\.0 %'
t_case "files outside the workspace are not counted"
t_assert_no_grep "${work}/out" registry

python3 "$gate" "${work}/cobertura.xml" --floor src=25 > "${work}/unfloored" 2>&1
t_case "the crate without a floor is named"
t_assert_grep "${work}/unfloored" '^Crate without a coverage floor: crates/observant$'
t_assert_grep "${work}/unfloored" '^crates/observant .* NO FLOOR$'

python3 "$gate" "${work}/cobertura.xml" --floor crates/observant=86 --floor src=23 --max-slack 3 > "${work}/slack" 2>&1
t_case "a slack over the limit is a warning naming the crate"
t_assert_grep "${work}/slack" '^warning: floor slack over 3 points: crates/observant 90\.0 % is 4\.0 points over its 86 % floor; raise the floor$'
t_case "a slack within the limit is not warned"
t_assert_no_grep "${work}/slack" 'warning: .* src '

python3 "$gate" "${work}/cobertura.xml" --floor crates/observant=87 --floor src=21 --max-slack 3 > "${work}/edge" 2>&1
t_case "a slack equal to the limit is not warned"
t_assert_no_grep "${work}/edge" 'crates/observant .* points over'
t_case "a slack just over the limit is warned"
t_assert_grep "${work}/edge" 'src 25\.0 % is 4\.0 points'

python3 "$gate" "${work}/cobertura.xml" --floor crates/observant=50 --floor src=10 > "${work}/noslack" 2>&1
t_case "without --max-slack nothing warns"
t_assert_no_grep "${work}/noslack" '^warning:'

t_summary
