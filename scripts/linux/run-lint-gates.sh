#!/usr/bin/env bash
# The hub lint aggregator on this repo's explicit root, as lint-gates.yml runs it; --exclude replaces third_party.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# Matches lint-gates.yml's `ratchets: true`; baselines are the <gate>.allow files, absent meaning zero.
antfrastructure_exec linux/scripts/run-lint-gates.sh "${KATAGLYPHIS_REPO_ROOT}" --ratchets "$@"
