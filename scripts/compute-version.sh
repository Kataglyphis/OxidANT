#!/usr/bin/env bash
set -euo pipefail

# compute-version.sh — thin wrapper.
#
# The logic (VERSION.txt -> REF_NAME -> RUN_NUMBER, plus the four-component
# MSIX form, plus the GITHUB_ENV/GITHUB_OUTPUT writes) lives in ContainerHub:
# every consumer with a CI lane needs exactly this, and it was reimplemented
# here before. Expects REF_NAME and RUN_NUMBER in the environment; writes
# VERSION and MSIX_VERSION for subsequent steps.
#
# BOTH paths this needs used to be CWD-RELATIVE literals, so the script only
# worked when the caller happened to sit in the repo root. The submodule path
# carried a hand-rolled [[ ! -f ]] guard, which at least failed loudly.
# VERSION.txt carried none - and version_util.sh reads a missing version file
# as "no version here", falling back to REF_NAME/RUN_NUMBER without a word in
# the log, so the wrong version ships instead of the lane failing.
#
# lib/containerhub.sh resolves the submodule from ${BASH_SOURCE[0]} and exports
# KATAGLYPHIS_REPO_ROOT, which anchors VERSION.txt to the same tree. Both work
# from any working directory now, and a missing submodule is named along with
# the command that fixes it.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=linux/lib/containerhub.sh
source "${SCRIPT_DIR}/linux/lib/containerhub.sh"

containerhub_exec linux/scripts/02-toolchain/rust/version_util.sh \
    --github-env "${KATAGLYPHIS_REPO_ROOT}/VERSION.txt"
