#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/linux/tests/lib-fake-gpu.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-fake-gpu.sh"
profile="${fake_gpu_tests_dir}/../profile-gpu-bundle.sh"
source "${fake_gpu_tests_dir}/../lib/ort-runtime-bundle.sh"
fake_gpu_workspace
for variant in cuda migraphx cpu; do
    bundle="${work}/${variant} bundle"
    fake_gpu_bundle_tree "${bundle}"
fake_gpu_write_launcher "${bundle}/oxidant" <<'LAUNCHER'
fake_gpu_assert_unset KATAGLYPHIS_ONNX_MODEL KATAGLYPHIS_ONNX_BACKEND ORT_DYLIB_PATH ORT_LIB_LOCATION
fake_gpu_assert_no_migraphx
test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"
fake_gpu_check_args "$@"
provider="$(fake_gpu_provider "${KATAGLYPHIS_ORT_DEVICE:-cpu}")"
: '--library-path "$here/lib/glibc:$here/lib"'
fake_gpu_node_profile "$3.json" "${provider}" "${FAKE_OPERATOR:-Conv}"
LAUNCHER
    if [ "${variant}" = cpu ]; then
        # A broken candidate selects CPU even when the verifier expects CUDA.
        sed -i '/^here=/a export LD_LIBRARY_PATH="${fake_gpu_tests_dir}/lib:/host/gpu-sdk"' "${bundle}/oxidant"
    else
        configure_gpu_launcher "${bundle}/oxidant" "${variant}"
    fi
done
fake_gpu_poison_env
fake_gpu_poison_migraphx
export KATAGLYPHIS_ORT_DEVICE=cpu
bash "${profile}" "${work}/cuda bundle" cuda "${work}/cuda"
bash "${profile}" "${work}/migraphx bundle" migraphx "${work}/amd"
mkdir -p "${work}/missing-hub/linux/scripts/lib"
fake_gpu_hub_refuses "${work}/missing-hub" 'lacks GPU profile environment isolation' 'Missing GPU profile environment helper was accepted' "${work}/missing.log" bash "${profile}" "${work}/cuda bundle" cuda "${work}/missing-helper"
printf '#!/usr/bin/env bash\n' > "${work}/missing-hub/linux/scripts/lib/gpu-profile-env.sh"
fake_gpu_hub_refuses "${work}/missing-hub" 'lacks GPU profile environment API' 'Missing GPU profile environment API was accepted' "${work}/missing.log" bash "${profile}" "${work}/cuda bundle" cuda "${work}/missing-api"
fake_gpu_refuses 'CPU-default launcher was accepted as CUDA' /dev/null bash "${profile}" "${work}/cpu bundle" cuda "${work}/cpu"
fake_gpu_refuses 'GPU transfer without model execution was accepted' /dev/null env FAKE_OPERATOR=MemcpyFromHost bash "${profile}" "${work}/cuda bundle" cuda "${work}/copy"
fake_gpu_refuses 'An unknown GPU device was accepted' "${work}/refused.log" bash "${profile}" "${work}/cuda bundle" tract "${work}/tract"
grep -q 'unsupported GPU device' "${work}/refused.log"
fake_gpu_refuses 'A missing argument was accepted' "${work}/refused.log" bash "${profile}" "${work}/cuda bundle"
grep -q '^usage: profile-gpu-bundle.sh' "${work}/refused.log"
rm "${work}/cuda bundle/resources/models/deim_dfine_n_coco.onnx"
fake_gpu_refuses 'missing bundled model was accepted' /dev/null bash "${profile}" "${work}/cuda bundle" cuda "${work}/missing-model"
printf 'GPU bundle profiling fixtures passed\n'
