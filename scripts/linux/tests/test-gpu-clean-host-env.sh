#!/usr/bin/env bash
# No actual package, GPU runtime or host installation is exercised.
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
cat > "$work/oxidant fixture" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
[[ -z "${KATAGLYPHIS_ONNX_MODEL+x}${KATAGLYPHIS_ONNX_BACKEND+x}${ORT_DYLIB_PATH+x}${ORT_LIB_LOCATION+x}${LD_LIBRARY_PATH+x}${LD_PRELOAD+x}${LD_AUDIT+x}" ]]
case "${KATAGLYPHIS_ORT_DEVICE:-}" in
  cpu) [[ "${FAIL_CPU:-0}" != 1 ]] ;;
  '') echo 'ORT device request: Migraphx'; echo 'libmigraphx_c.so: cannot open'; exit 1 ;;
  *) echo 'Unexpected inherited device' >&2; exit 1 ;;
esac
STUB
chmod +x "$work/oxidant fixture"
export GITHUB_WORKSPACE="$root" KATAGLYPHIS_ONNX_MODEL=/wrong KATAGLYPHIS_ONNX_BACKEND=tract
export ORT_DYLIB_PATH=/wrong ORT_LIB_LOCATION=/wrong LD_LIBRARY_PATH=/wrong
export LD_PRELOAD=/wrong LD_AUDIT=/wrong KATAGLYPHIS_ORT_DEVICE=cuda
bash "$root/scripts/linux/check-gpu-clean-host.sh" amd "$work/oxidant fixture" 2>"$work/stderr"
if FAIL_CPU=1 bash "$root/scripts/linux/check-gpu-clean-host.sh" amd "$work/oxidant fixture" >/dev/null 2>&1; then
  echo 'A failed packaged CPU check was accepted' >&2; exit 1
fi
echo 'Poisoned host overrides are cleared for the AMD clean-host checks'
