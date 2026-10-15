#!/usr/bin/env bash
# The per-crate coverage floor on a synthetic cobertura.xml: each crate graded alone, and every way it must go red.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
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

passed=0
# expect <rc> <name> <args...>: the gate's exit code for these arguments.
expect() {
  local want="$1" name="$2" got=0
  shift 2
  python3 "$gate" "$@" > "${work}/out" 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    printf 'FAIL %s: expected rc %s, got %s\n' "$name" "$want" "$got" >&2
    sed 's/^/  | /' "${work}/out" >&2
    return 1
  fi
  passed=$((passed + 1))
  printf 'ok   %s\n' "$name"
}

expect 0 "both crates over their floors" "${work}/cobertura.xml" --floor crates/observant=90 --floor src=25
expect 1 "the root crate under its floor" "${work}/cobertura.xml" --floor crates/observant=90 --floor src=26
expect 1 "the good crate does not carry the bad one" "${work}/cobertura.xml" --floor crates/observant=50 --floor src=50
expect 1 "a floor for a crate with no lines" "${work}/cobertura.xml" --floor crates/telemetry=1
expect 2 "no floor at all" "${work}/cobertura.xml"
expect 2 "a malformed floor" "${work}/cobertura.xml" --floor src=half

python3 "$gate" "${work}/cobertura.xml" --floor src=0 > "${work}/out"
grep -qE '^crates/observant +9 / +10 +90\.0 %' "${work}/out" || { echo "FAIL the table counts lines per crate" >&2; cat "${work}/out" >&2; exit 1; }
passed=$((passed + 1)); echo "ok   the table counts lines per crate"
if grep -q registry "${work}/out"; then echo "FAIL a file outside the workspace was counted" >&2; exit 1; fi
passed=$((passed + 1)); echo "ok   files outside the workspace are not counted"

printf '  %d assertion(s) passed\n' "$passed"
