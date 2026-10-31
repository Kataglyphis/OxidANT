#!/usr/bin/env bash
# Remove host overrides before checking a packaged GPU variant on CPU.
set -euo pipefail
if [[ $# -ne 2 ]]; then echo 'usage: check-gpu-clean-host.sh nvidia|amd EXECUTABLE' >&2; exit 2; fi
case "$1" in nvidia|amd) ;; *) echo 'unsupported GPU vendor' >&2; exit 2 ;; esac
exec env -u KATAGLYPHIS_ONNX_MODEL -u KATAGLYPHIS_ONNX_BACKEND -u ORT_DYLIB_PATH -u ORT_LIB_LOCATION \
  -u LD_LIBRARY_PATH -u LD_PRELOAD -u LD_AUDIT KATAGLYPHIS_ORT_DEVICE=cpu \
  bash -c 'set -euo pipefail
    "$2" onnx-check
    "$2" media-check
    bash "$GITHUB_WORKSPACE/scripts/linux/check-missing-gpu-runtime.sh" "$1" "$2"' _ "$1" "$2"
