#!/usr/bin/env bash
# A clean host must report the selected provider's missing runtime, not silently use CPU.
set -euo pipefail
if [ "$#" -lt 2 ]; then
    echo 'usage: check-missing-gpu-runtime.sh nvidia|amd EXECUTABLE [ARG ...]' >&2
    exit 2
fi
vendor="$1"
shift
case "${vendor}" in
    nvidia) device=Cuda; missing='lib(cuda|cudart|cublas|curand|cudnn|cufft|cusparse|nvrtc|nvJitLink)' ;;
    amd) device=Migraphx; missing='lib(migraphx_c|amdhip64)\.so' ;;
    *) printf 'unsupported GPU vendor: %s\n' "${vendor}" >&2; exit 2 ;;
esac
log="$(mktemp)"
trap 'rm -f -- "${log}"' EXIT
if env -u KATAGLYPHIS_ORT_DEVICE -u KATAGLYPHIS_ONNX_BACKEND -u KATAGLYPHIS_ONNX_MODEL -u ORT_DYLIB_PATH RUST_LOG=info "$@" onnx-check >"${log}" 2>&1; then
    printf '%s GPU inference unexpectedly succeeded without the host runtime\n' "${vendor}" >&2
    exit 1
fi
cat "${log}"
grep -Fq "ORT device request: ${device}" "${log}" || { echo 'GPU package did not select its expected device' >&2; exit 1; }
grep -Eiq "${missing}[^[:space:]]*.*(cannot open|not found|no such file)" "${log}" || {
    echo 'Failure did not identify the missing GPU runtime' >&2; exit 1;
}
