#!/usr/bin/env bash
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
profile="${here}/../profile-gpu-bundle.sh"
source "${here}/../lib/ort-runtime-bundle.sh"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
for variant in cuda migraphx cpu; do
    bundle="${work}/${variant} bundle"
    mkdir -p "${bundle}/resources/models" "${bundle}/lib"
    printf model > "${bundle}/resources/models/deim_dfine_n_coco.onnx"
    printf runtime > "${bundle}/lib/libonnxruntime.so"
cat > "${bundle}/oxidant" <<'LAUNCHER'
#!/bin/bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
test -z "${KATAGLYPHIS_ONNX_MODEL+x}${KATAGLYPHIS_ONNX_BACKEND+x}${ORT_DYLIB_PATH+x}${ORT_LIB_LOCATION+x}"
test -z "${ORT_MIGRAPHX_FP16_ENABLE+x}${ORT_MIGRAPHX_MODEL_CACHE_PATH+x}${MIGRAPHX_SKIP_BENCHMARKING+x}${MIGRAPHX_UNKNOWN_OVERRIDE+x}"
test "${LD_LIBRARY_PATH}" = "${here}/lib:/host/gpu-sdk"
test "$#" -eq 3 && test "$1" = onnx-check && test "$2" = --profile
case "${KATAGLYPHIS_ORT_DEVICE:-cpu}" in
    cuda) provider=CUDAExecutionProvider ;;
    migraphx) provider=MIGraphXExecutionProvider ;;
    cpu) provider=CPUExecutionProvider ;;
esac
: '--library-path "$here/lib/glibc:$here/lib"'
printf '[{"cat":"Node","args":{"provider":"%s","op_name":"%s"}}]\n' "${provider}" "${FAKE_OPERATOR:-Conv}" > "$3.json"
LAUNCHER
    chmod +x "${bundle}/oxidant"
    if [ "${variant}" = cpu ]; then
        # A broken candidate selects CPU even when the verifier expects CUDA.
        sed -i '/^here=/a export LD_LIBRARY_PATH="$here/lib:/host/gpu-sdk"' "${bundle}/oxidant"
    else
        configure_gpu_launcher "${bundle}/oxidant" "${variant}"
    fi
done
export KATAGLYPHIS_ONNX_MODEL=/wrong KATAGLYPHIS_ONNX_BACKEND=tract KATAGLYPHIS_ORT_DEVICE=cpu
export ORT_DYLIB_PATH=/wrong ORT_LIB_LOCATION=/wrong LD_LIBRARY_PATH=/host/gpu-sdk
export ORT_MIGRAPHX_FP16_ENABLE=1 ORT_MIGRAPHX_MODEL_CACHE_PATH=/wrong MIGRAPHX_SKIP_BENCHMARKING=1 MIGRAPHX_UNKNOWN_OVERRIDE=poison
bash "${profile}" "${work}/cuda bundle" cuda "${work}/cuda"
bash "${profile}" "${work}/migraphx bundle" migraphx "${work}/amd"
mkdir -p "${work}/missing-hub/linux/scripts/lib"
if ANTFRASTRUCTURE_DIR="${work}/missing-hub" bash "${profile}" "${work}/cuda bundle" cuda "${work}/missing-helper" >"${work}/missing.log" 2>&1; then
    echo 'Missing GPU profile environment helper was accepted' >&2; exit 1
fi
grep -q 'lacks GPU profile environment isolation' "${work}/missing.log"
printf '#!/usr/bin/env bash\n' > "${work}/missing-hub/linux/scripts/lib/gpu-profile-env.sh"
if ANTFRASTRUCTURE_DIR="${work}/missing-hub" bash "${profile}" "${work}/cuda bundle" cuda "${work}/missing-api" >"${work}/missing.log" 2>&1; then
    echo 'Missing GPU profile environment API was accepted' >&2; exit 1
fi
grep -q 'lacks GPU profile environment API' "${work}/missing.log"
if bash "${profile}" "${work}/cpu bundle" cuda "${work}/cpu" >/dev/null 2>&1; then
    echo 'CPU-default launcher was accepted as CUDA' >&2; exit 1
fi
if FAKE_OPERATOR=MemcpyFromHost bash "${profile}" "${work}/cuda bundle" cuda "${work}/copy" >/dev/null 2>&1; then
    echo 'GPU transfer without model execution was accepted' >&2; exit 1
fi
rm "${work}/cuda bundle/resources/models/deim_dfine_n_coco.onnx"
if bash "${profile}" "${work}/cuda bundle" cuda "${work}/missing-model" >/dev/null 2>&1; then
    echo 'missing bundled model was accepted' >&2; exit 1
fi
printf 'GPU bundle profiling fixtures passed\n'
