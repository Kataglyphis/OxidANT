#!/usr/bin/env bash
# Freezes the Rust functions over 80 lines in the gated crates: #[expect(clippy::too_many_lines)] stops new ones, not growth.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
hub=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --root) root="$(cd "$2" && pwd)"; shift 2 ;;
    --hub) hub="$2"; shift 2 ;;
    *) printf 'usage: check-rust-function-sizes.sh [--root DIR] [--hub DIR]\n' >&2; exit 2 ;;
  esac
done
hub="${hub:-${root}/third_party/ANTfrastructure}"
allow="${root}/rust-function-size.allow"
# The crates whose roots carry #![warn(clippy::too_many_lines)].
scopes=(crates/observant crates/inference crates/media src)

scan_args=()
for scope in "${scopes[@]}"; do [ -d "${root}/${scope}" ] && scan_args+=(--scan "${scope}"); done
measured="$(python3 "${hub}/linux/scripts/verify_code_size.py" --root "${root}" --report-only --extra-languages \
  "${scan_args[@]}" | sed -n 's/^REPORT-ONLY: rust function \([^:]*\):[0-9]*:\([^ ]*\) = \([0-9]*\) lines$/\1 | \2 | \3/p')"

MEASURED="${measured}" python3 - "${allow}" <<'PY'
import os
import sys
frozen = {}
for line in open(sys.argv[1], encoding="utf-8"):
    if line.strip() and not line.startswith("#"):
        path, name, lines, *_ = [cell.strip() for cell in line.split("|")]
        frozen[(path, name)] = int(lines)
failed = False
for row in os.environ["MEASURED"].splitlines():
    path, name, lines = [cell.strip() for cell in row.split("|")]
    budget = frozen.pop((path, name), None)
    if budget is None:
        print(f"NEW: {path} {name} has {lines} lines, over 80: split it")
        failed = True
    elif int(lines) > budget:
        print(f"GREW: {path} {name} from {budget} to {lines} lines: split it, do not raise the budget")
        failed = True
    elif int(lines) < budget:
        print(f"note: {path} {name} shrank from {budget} to {lines}; lower its row in rust-function-size.allow")
for path, name in frozen:
    print(f"note: {path} {name} is 80 lines or fewer now; drop its row and its #[expect]")
print("rust function sizes: FAILED" if failed else "rust function sizes: OK")
sys.exit(1 if failed else 0)
PY
