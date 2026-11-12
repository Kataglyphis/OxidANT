#!/usr/bin/env bash
# The hub lint aggregator on this repo's explicit root, as lint-gates.yml runs it; --exclude replaces third_party.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# Matches lint-gates.yml's `workflow-conventions: sha-pin`.
export WORKFLOW_CONVENTIONS_GATE="${WORKFLOW_CONVENTIONS_GATE:-sha-pin}"

# First, and in a second: a refactor that moves a line a mutation recipe names fails here, not in CI after the push.
stale_check="$(antfrastructure_path docs/scripts/verify_mutations.py)"
for manifest in "${KATAGLYPHIS_REPO_ROOT}"/scripts/linux/tests/*mutations*.json; do
  python3 "${stale_check}" --manifest "${manifest}" --root "${KATAGLYPHIS_REPO_ROOT}" --stale-check
done

# Matches lint-gates.yml's `ratchets: true` and `agents-drift: true`; baselines are the <gate>.allow files, absent meaning zero.
antfrastructure_exec linux/scripts/run-lint-gates.sh "${KATAGLYPHIS_REPO_ROOT}" --ratchets --agents-drift "$@"
