#!/usr/bin/env bash
# The renderer owns its Slang inputs; ANTfrastructure owns compilation and WGSL validation.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/slang-compile.sh
SLANG_COMPILE_SOURCE_ROOT="${KATAGLYPHIS_REPO_ROOT}/shaders"
export SLANG_COMPILE_MANIFEST="${SLANG_COMPILE_SOURCE_ROOT}/shader-manifest.json"
SLANG_COMPILE_DEST_ROOT="${KATAGLYPHIS_REPO_ROOT}"
case "${1:-}" in
    --check)
        SLANG_COMPILE_DEST_ROOT="$(mktemp -d)"
        trap 'rm -rf "${SLANG_COMPILE_DEST_ROOT}"' EXIT
        ;;
    '') ;;
    *) echo 'usage: compile-slang-shaders.sh [--check]' >&2; exit 2 ;;
esac
slang_compile_main
if [[ "${1:-}" == --check ]]; then
    python3 "${KATAGLYPHIS_REPO_ROOT}/scripts/check-shader-output.py" "${SLANG_COMPILE_DEST_ROOT}"
fi
