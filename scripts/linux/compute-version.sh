#!/usr/bin/env bash
set -euo pipefail

# Wraps the hub's version_util.sh; VERSION.txt is root-anchored, since a missing one silently falls back to REF_NAME.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

antfrastructure_exec linux/scripts/02-toolchain/rust/version_util.sh \
    --github-env "${KATAGLYPHIS_REPO_ROOT}/VERSION.txt"
