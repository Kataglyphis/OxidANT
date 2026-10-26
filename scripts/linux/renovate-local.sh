#!/usr/bin/env bash
# Runs the hub's Renovate CLI on this repo's root; see third_party/ANTfrastructure/docs/dependency-updates.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# The copied bootstrap initializes the hub only for a real apply.
antfrastructure_renovate_exec linux/scripts/renovate-local.sh "$KATAGLYPHIS_REPO_ROOT" "$@"
# Lock maintenance can advance transitive Burn crates even when Renovate defers the direct pin.
if antfrastructure_renovate_will_apply "$@"; then
    python3 "${SCRIPT_DIR}/check-burn-lock-graph.py" "${KATAGLYPHIS_REPO_ROOT}/Cargo.lock"
fi
