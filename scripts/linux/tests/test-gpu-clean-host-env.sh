#!/usr/bin/env bash
# No actual package, GPU runtime or host installation is exercised.
set -euo pipefail
# shellcheck source=scripts/linux/tests/lib-fake-gpu.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-fake-gpu.sh"
root="$fake_gpu_repo"
fake_gpu_workspace
fake_gpu_write_launcher "$work/oxidant fixture" <<'STUB'
fake_gpu_assert_unset KATAGLYPHIS_ONNX_MODEL KATAGLYPHIS_ONNX_BACKEND ORT_DYLIB_PATH ORT_LIB_LOCATION LD_LIBRARY_PATH LD_PRELOAD LD_AUDIT
case "${KATAGLYPHIS_ORT_DEVICE:-}" in
  cpu) [[ "${FAIL_CPU:-0}" != 1 ]] ;;
  '') echo 'ORT device request: Migraphx'; echo 'libmigraphx_c.so: cannot open'; exit 1 ;;
  *) echo 'Unexpected inherited device' >&2; exit 1 ;;
esac
STUB
fake_gpu_poison_env
export GITHUB_WORKSPACE="$root" LD_LIBRARY_PATH=/wrong LD_PRELOAD=/wrong LD_AUDIT=/wrong KATAGLYPHIS_ORT_DEVICE=cuda
bash "$root/scripts/linux/check-gpu-clean-host.sh" amd "$work/oxidant fixture" 2>"$work/stderr"
fake_gpu_refuses 'A failed packaged CPU check was accepted' /dev/null env FAIL_CPU=1 bash "$root/scripts/linux/check-gpu-clean-host.sh" amd "$work/oxidant fixture"
echo 'Poisoned host overrides are cleared for the AMD clean-host checks'
