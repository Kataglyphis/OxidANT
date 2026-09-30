#!/usr/bin/env bash
# Runs the hub's Renovate CLI on this repo's root; see third_party/ANTfrastructure/docs/dependency-updates.md.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# The likely failure is a pin predating the driver, so name it instead of the generic not-found hint.
HUB_RENOVATE_RELATIVE="linux/scripts/renovate-local.sh"
if [ ! -f "${ANTFRASTRUCTURE_DIR}/${HUB_RENOVATE_RELATIVE}" ]; then
  echo "Error: ${ANTFRASTRUCTURE_DIR}/${HUB_RENOVATE_RELATIVE} is missing." >&2
  echo "       Either ANTfrastructure is not checked out (git submodule update" >&2
  echo "       --init --recursive third_party/ANTfrastructure), or the pinned" >&2
  echo "       ANTfrastructure predates the shared Renovate CLI - bump the" >&2
  echo "       third_party/ANTfrastructure gitlink." >&2
  exit 1
fi

antfrastructure_exec "${HUB_RENOVATE_RELATIVE}" "${KATAGLYPHIS_REPO_ROOT}" "$@"
