#!/usr/bin/env bash
# Fixture parts for the GPU check suites and the fake launchers they write; it proves nothing about a real provider.
fake_gpu_tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fake_gpu_repo="$(cd "${fake_gpu_tests_dir}/../../.." && pwd)"
# A launcher runs with env -u on the overrides, so only this path tells it where these helpers live.
export FAKE_GPU_LIB="${fake_gpu_tests_dir}/lib-fake-gpu.sh"

fake_gpu_hub_dir() {
    printf '%s\n' "${ANTFRASTRUCTURE_DIR:-${fake_gpu_repo}/third_party/ANTfrastructure}"
}

# Publishes work; the EXIT trap outlives this function, so the suite keeps its cleanup.
fake_gpu_workspace() {
    work="$(mktemp -d)"
    trap 'rm -rf -- "${work}"' EXIT
}

# Every override the checks must clear reaches the launcher unless the check removes it.
fake_gpu_poison_env() {
    export KATAGLYPHIS_ONNX_MODEL=/wrong KATAGLYPHIS_ONNX_BACKEND=tract ORT_DYLIB_PATH=/wrong ORT_LIB_LOCATION=/wrong
    export ORT_AMDGPU_EP_ROOT=/wrong
    export LD_LIBRARY_PATH=/host/gpu-sdk LD_PRELOAD='' LD_AUDIT=''
}

fake_gpu_poison_migraphx() {
    export ORT_MIGRAPHX_FP16_ENABLE=1 ORT_MIGRAPHX_MODEL_CACHE_PATH=/wrong
    export MIGRAPHX_SKIP_BENCHMARKING=1 MIGRAPHX_UNKNOWN_OVERRIDE=poison
}

# fake_gpu_refuses MESSAGE LOG COMMAND...: the run must fail; LOG keeps its output for the caller to read.
fake_gpu_refuses() {
    local message="$1" log="$2"
    shift 2
    if "$@" >"${log}" 2>&1; then
        printf '%s\n' "${message}" >&2
        exit 1
    fi
}

# fake_gpu_accepts MESSAGE COMMAND...: a mutant must still pass, or the mutation proves nothing.
fake_gpu_accepts() {
    local message="$1"
    shift
    "$@" >/dev/null 2>&1 || { printf '%s\n' "${message}" >&2; exit 1; }
}

# fake_gpu_hub_refuses HUB PATTERN MESSAGE LOG COMMAND...: a hub lacking a tool refuses and names it.
fake_gpu_hub_refuses() {
    local hub="$1" pattern="$2" message="$3" log="$4"
    shift 4
    fake_gpu_refuses "${message}" "${log}" env ANTFRASTRUCTURE_DIR="${hub}" "$@"
    grep -q "${pattern}" "${log}"
}

fake_gpu_bundle_tree() {
    mkdir -p "$1/resources/models" "$1/lib"
    printf model > "$1/resources/models/deim_dfine_n_coco.onnx"
    printf runtime > "$1/lib/libonnxruntime.so"
}

# fake_gpu_write_launcher PATH: the common launcher start, then stdin; the bundle rewrite needs the literal shebang and here=.
fake_gpu_write_launcher() {
    {
        cat <<'HEAD'
#!/bin/bash
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
source "${FAKE_GPU_LIB}"
HEAD
        cat
    } > "$1"
    chmod +x "$1"
}

# Launcher helpers: a failed expectation exits, as a launcher's own set -e would.
fake_gpu_assert_unset() {
    local name
    for name in "$@"; do
        [[ -z ${!name+x} ]] || exit 1
    done
}

fake_gpu_assert_no_migraphx() {
    fake_gpu_assert_unset ORT_MIGRAPHX_FP16_ENABLE ORT_MIGRAPHX_MODEL_CACHE_PATH MIGRAPHX_SKIP_BENCHMARKING MIGRAPHX_UNKNOWN_OVERRIDE
}

fake_gpu_check_args() {
    [[ $# -eq 3 && $1 == onnx-check && $2 == --profile ]] || exit 1
}

fake_gpu_detect_args() {
    [[ $# -eq 9 && $2 == --image && $4 == --output && $6 == --profile && $8 == --repeat-output ]] || exit 1
}

fake_gpu_provider() {
    case "$1" in
        cpu) printf CPUExecutionProvider ;;
        cuda) printf CUDAExecutionProvider ;;
        migraphx) printf MIGraphX ;;
        *) return 1 ;;
    esac
}

# The name the hub's checker at this pin expects: the suites prove the drivers' wiring, the hub proves its own table.
fake_gpu_hub_provider() {
    python3 -I -c 'import runpy, sys; print(runpy.run_path(sys.argv[1])["PROVIDERS"][sys.argv[2]], end="")' \
        "$(fake_gpu_hub_dir)/shared/python/verify_ort_profile.py" "$1"
}

fake_gpu_node_profile() {
    printf '[{"cat":"Node","args":{"provider":"%s","op_name":"%s"}}]\n' "$2" "$3" > "$1"
}

fake_gpu_two_run_profile() {
    {
        printf '[{"cat":"Session","name":"model_run","ph":"X","pid":1,"ts":0,"dur":90},{"cat":"Node","ph":"X","pid":1,"ts":10,"dur":20,"args":{"provider":"%s","op_name":"Conv"}}' "$2"
        printf ',{"cat":"Session","name":"model_run","ph":"X","pid":1,"ts":100,"dur":90},{"cat":"Node","ph":"X","pid":1,"ts":110,"dur":20,"args":{"provider":"%s","op_name":"Conv"}}' "$3"
        printf ']\n'
    } > "$1"
}

# A GPU run scores 0.6 when the two values match, so the parity comparison has a real drift to catch.
fake_gpu_drift_score() {
    if [[ $1 == "$2" && ${KATAGLYPHIS_ORT_DEVICE} != cpu ]]; then printf 0.6; else printf '%s' "$3"; fi
}

fake_gpu_detections() {
    printf '{"schema_version":1,"width":100,"height":100,"detections":[{"class":15,"score":%s,"box":[10,60,30,95]},{"class":15,"score":0.7,"box":[50,20,80,60]}]}' "$2" > "$1"
}
