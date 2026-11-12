#!/usr/bin/env bash
# Argument, hub-tool, package and profile checks shared by the Linux GPU check scripts; it runs no GPU check itself.

gpu_device_known() {
    case "$1" in
        cuda|migraphx) return 0 ;;
        *) return 1 ;;
    esac
}

gpu_check_argc() {
    local actual="$1" least="$2" most="$3" usage="$4"
    if [ "${actual}" -lt "${least}" ] || [ "${actual}" -gt "${most}" ]; then
        printf '%s\n' "${usage}" >&2
        exit 2
    fi
}

gpu_check_require_device() {
    gpu_device_known "$1" || { echo 'unsupported GPU device' >&2; exit 2; }
}

gpu_check_launcher() {
    local launcher
    launcher="$(realpath -e "$1")"
    [[ -x "${launcher}" ]] || { echo 'missing launcher or fixture' >&2; exit 1; }
    printf '%s\n' "${launcher}"
}

gpu_check_fixture() {
    local fixture
    fixture="$(realpath -e "$1")"
    [[ -f "${fixture}" ]] || { echo 'missing launcher or fixture' >&2; exit 1; }
    printf '%s\n' "${fixture}"
}

# Host overrides that could redirect a packaged launcher away from its own runtime, plugin module and model.
gpu_check_clean_env_args() {
    printf '%s ' env -u LD_PRELOAD -u LD_AUDIT -u KATAGLYPHIS_ONNX_MODEL -u KATAGLYPHIS_ONNX_BACKEND \
        -u ORT_DYLIB_PATH -u ORT_LIB_LOCATION -u ORT_AMDGPU_EP_ROOT
}

gpu_check_hub_dir() {
    local repo
    repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
    printf '%s\n' "${ANTFRASTRUCTURE_DIR:-${repo}/third_party/ANTfrastructure}"
}

# gpu_check_hub_tool RELATIVE_PATH MESSAGE: the tool's path; a pinned hub without it refuses with MESSAGE.
gpu_check_hub_tool() {
    local tool
    tool="$(gpu_check_hub_dir)/$1"
    [[ -f "${tool}" ]] || { printf '%s\n' "$2" >&2; exit 1; }
    printf '%s\n' "${tool}"
}

gpu_check_comparator() {
    gpu_check_hub_tool shared/python/compare_detector_parity.py "$1"
}

gpu_check_profile_tool() {
    gpu_check_hub_tool shared/python/verify_ort_profile.py "$1"
}

gpu_check_load_profile_env() {
    local helper
    helper="$(gpu_check_hub_tool linux/scripts/lib/gpu-profile-env.sh 'Pinned ANTfrastructure lacks GPU profile environment isolation')"
    # shellcheck source=/dev/null
    source "${helper}"
    declare -F gpu_without_migraphx_overrides >/dev/null || {
        echo 'Pinned ANTfrastructure lacks GPU profile environment API' >&2
        exit 1
    }
}

gpu_check_workspace() {
    mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/oxidant-$1.XXXXXXXX"
}

# Call these two in a substitution, so nullglob stays local and the refusal only stops the substitution's assignment.
gpu_check_one_match() {
    local -a found=()
    shopt -s nullglob
    found=("$1"*"$2")
    [ "${#found[@]}" -eq 1 ] || { printf '%s\n' "$3" >&2; exit 1; }
    printf '%s\n' "${found[0]}"
}

gpu_check_one_package() {
    gpu_check_one_match "$1/" "$2" 'Expected exactly one tarball, DEB and AppImage'
}

gpu_check_one_profile() {
    gpu_check_one_match "$1" .json "Expected one $2 profile"
}
