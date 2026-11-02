#!/usr/bin/env bash
# Remove host overrides before checking a packaged GPU variant on CPU.
set -euo pipefail
# shellcheck source=lib/gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/gpu-check-common.sh"
gpu_check_argc "$#" 2 2 'usage: check-gpu-clean-host.sh nvidia|amd EXECUTABLE'
case "$1" in nvidia|amd) ;; *) echo 'unsupported GPU vendor' >&2; exit 2 ;; esac
read -r -a clean <<< "$(gpu_check_clean_env_args)"
exec "${clean[@]}" -u LD_LIBRARY_PATH KATAGLYPHIS_ORT_DEVICE=cpu \
  bash -c 'set -euo pipefail
    "$2" onnx-check
    "$2" media-check
    bash "$GITHUB_WORKSPACE/scripts/linux/check-missing-gpu-runtime.sh" "$1" "$2"' _ "$1" "$2"
