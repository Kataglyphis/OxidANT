#!/usr/bin/env bash
# Verify both the selected device and the reason inference fails on a clean host.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf -- "${work}"' EXIT
script="${here}/../check-missing-gpu-runtime.sh"
cat > "${work}/program with spaces" <<'PROGRAM'
#!/usr/bin/env bash
set -euo pipefail
test "${1:-}" = "preserved argument"
test "${2:-}" = "onnx-check"
test -z "${KATAGLYPHIS_ORT_DEVICE+x}${KATAGLYPHIS_ONNX_BACKEND+x}${KATAGLYPHIS_ONNX_MODEL+x}${ORT_DYLIB_PATH+x}"
test "${RUST_LOG}" = info
case "${SCENARIO}" in
    nvidia) echo 'ORT device request: Cuda'; echo 'libcudart.so.13: cannot open'; exit 1 ;;
    amd) echo 'ORT device request: Migraphx'; echo 'libmigraphx_c.so.2: cannot open'; exit 1 ;;
    amd-hip) echo 'ORT device request: Migraphx'; echo 'libamdhip64.so.7: cannot open'; exit 1 ;;
    cpu) echo 'ORT device request: Cpu'; echo 'libcudart.so.13: cannot open'; exit 1 ;;
    unrelated) echo 'ORT device request: Cuda'; echo 'model unavailable'; exit 1 ;;
    misleading) echo 'ORT device request: Cuda'; echo 'libcudart.so.13 loaded'; echo 'model unavailable'; exit 1 ;;
    success) exit 0 ;;
esac
PROGRAM
chmod +x "${work}/program with spaces"
export KATAGLYPHIS_ORT_DEVICE=cpu KATAGLYPHIS_ONNX_BACKEND=tract KATAGLYPHIS_ONNX_MODEL=/wrong ORT_DYLIB_PATH=/wrong
check() { SCENARIO="$1" bash "${script}" "$2" "${work}/program with spaces" 'preserved argument' >/dev/null 2>&1; }
check nvidia nvidia
check amd amd
check amd-hip amd
for scenario in cpu unrelated misleading success; do
    if check "${scenario}" nvidia; then printf 'accepted invalid scenario: %s\n' "${scenario}" >&2; exit 1; fi
done
if check nvidia unsupported; then echo 'accepted unsupported vendor' >&2; exit 1; fi
printf 'Missing GPU runtime checks passed\n'
