#!/usr/bin/env bash
set -euo pipefail

# Wraps the hub's version_util.sh on Cargo.toml's [workspace.package] version, root-anchored: a missing file falls back to REF_NAME.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

antfrastructure_exec linux/scripts/02-toolchain/rust/version_util.sh \
    --github-env "${KATAGLYPHIS_REPO_ROOT}/Cargo.toml"
