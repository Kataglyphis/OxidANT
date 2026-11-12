#!/usr/bin/env bash
# The hub stages the chain ORT core and its GPU module; this file adds the GPU launcher rewrite and the AMD plugin's own texts.
# shellcheck source=antfrastructure.sh
source "$(dirname "${BASH_SOURCE[0]}")/antfrastructure.sh" || return $?
# shellcheck source=gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/gpu-check-common.sh" || return $?
# shellcheck source=/dev/null
source "$(antfrastructure_path linux/scripts/06-packaging/ort-runtime-bundle.sh)" || return $?

# The hub has no helper for the plugin's texts; they travel as one tree, so a seed's name cannot meet another notice directory.
ORT_AMDGPU_EP_NOTICES=licenses/onnxruntime-ep-amdgpu

# <bundle>: every licence text beside the image's plugin module, and its provenance.
stage_amdgpu_ep_notices() {
    local bundle="${1:?bundle required}" root
    root="$(dirname "$(ort_amdgpu_ep_dir)")"
    if [ ! -s "${root}/provenance.txt" ] || [ -z "$(find "${root}/licenses" -type f -print -quit 2>/dev/null)" ]; then
        printf 'the MIGraphX plugin has no licence texts or provenance under %s\n' "${root}" >&2
        return 1
    fi
    mkdir -p "${bundle}/${ORT_AMDGPU_EP_NOTICES}" || return
    cp -RL "${root}/licenses/." "${bundle}/${ORT_AMDGPU_EP_NOTICES}/" || return
    cp -L "${root}/provenance.txt" "${bundle}/${ORT_AMDGPU_EP_NOTICES}/provenance.txt"
}

# <bundle>: its plugin texts are the image's, file for file, and none is a link.
verify_amdgpu_ep_notices() {
    local bundle="${1:?bundle required}" expected
    expected="$(mktemp -d)" || return 1
    if stage_amdgpu_ep_notices "${expected}" &&
        [ -z "$(find "${bundle}/${ORT_AMDGPU_EP_NOTICES}" -type l -print -quit 2>/dev/null)" ] &&
        diff -r "${expected}/${ORT_AMDGPU_EP_NOTICES}" "${bundle}/${ORT_AMDGPU_EP_NOTICES}" >&2; then
        rm -rf -- "${expected}"
        return 0
    fi
    rm -rf -- "${expected}"
    printf "the package's MIGraphX plugin texts are not the image's: %s\n" "${bundle}/${ORT_AMDGPU_EP_NOTICES}" >&2
    return 1
}

configure_gpu_launcher() {
    local launcher="${1:?launcher required}" device="${2:?device required}" line found=0 found_here=0
    # shellcheck disable=SC2016
    local old='--library-path "$here/lib/glibc:$here/lib"'
    # shellcheck disable=SC2016
    local new='--library-path "$here/lib/glibc:$here/lib${gpu_library_path}"'
    gpu_device_known "${device}" || { printf 'unknown GPU device: %s\n' "${device}" >&2; return 2; }
    {
        IFS= read -r line
        [ "${line}" = '#!/bin/bash' ] || { printf 'unexpected bundle launcher: %s\n' "${launcher}" >&2; return 1; }
        printf '%s\n' "${line}"
        # shellcheck disable=SC2016
        printf 'export KATAGLYPHIS_ORT_DEVICE="${KATAGLYPHIS_ORT_DEVICE:-%s}"\n' "${device}"
        # shellcheck disable=SC2016
        printf '%s\n' 'gpu_library_path=' 'IFS=: read -r -a gpu_library_dirs <<< "${LD_LIBRARY_PATH:-}"' \
            'for gpu_library_dir in "${gpu_library_dirs[@]}"; do' \
            '  [ -z "${gpu_library_dir}" ] || gpu_library_path="${gpu_library_path}:${gpu_library_dir}"' 'done'
        while IFS= read -r line || [ -n "${line}" ]; do
            case "${line}" in *"${old}"*) line="${line/"${old}"/"${new}"}"; found=1 ;; esac
            printf '%s\n' "${line}"
            case "${line}" in
                'here='*)
                    found_here=1
                    # shellcheck disable=SC2016
                    printf '%s\n' 'export LD_LIBRARY_PATH="$here/lib${gpu_library_path}"'
                    ;;
            esac
        done
        [ "${found}" -eq 1 ] && [ "${found_here}" -eq 1 ] || { printf 'bundle launcher has unexpected paths: %s\n' "${launcher}" >&2; return 1; }
    } < "${launcher}" > "${launcher}.gpu"
    chmod 0755 "${launcher}.gpu"
    mv "${launcher}.gpu" "${launcher}"
}
