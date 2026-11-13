#!/usr/bin/env bash
# The script suites of lint-gates.yml in one file, which the hub's pre-push hook runs too: a red suite stops the push.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
for suite in scripts/linux/tests/test-*.sh; do bash "${suite}"; done
bash scripts/linux/check-rust-function-sizes.sh
python3 scripts/linux/tests/test-detector-parity.py
