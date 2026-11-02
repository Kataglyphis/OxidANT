#!/usr/bin/env bash
# The hub stages the chain ORT core and provider; this file adds only the GPU launcher rewrite.
# shellcheck source=antfrastructure.sh
source "$(dirname "${BASH_SOURCE[0]}")/antfrastructure.sh" || return $?
# shellcheck source=gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/gpu-check-common.sh" || return $?
# shellcheck source=/dev/null
source "$(antfrastructure_path linux/scripts/06-packaging/ort-runtime-bundle.sh)" || return $?

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
