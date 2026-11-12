#!/usr/bin/env bash
# Check the finished Linux GPU packages before uploading a candidate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${script_dir}/lib/antfrastructure.sh"
# shellcheck source=lib/models-lock.sh
source "${script_dir}/lib/models-lock.sh"
# shellcheck source=lib/gpu-check-common.sh
source "${script_dir}/lib/gpu-check-common.sh"
if [ "$#" -ne 3 ]; then
    printf 'Usage: %s PACKAGE_DIR x64|arm64 nvidia|amd\n' "$0" >&2
    exit 2
fi
package_dir="$(realpath -e "$1")"
case "$2" in x64|arm64) arch="$2" ;; *) echo 'unsupported package architecture' >&2; exit 2 ;; esac
if [ "$3" = amd ] && [ "$arch" = arm64 ]; then
    echo 'Linux arm64 ROCm packages are not supported' >&2
    exit 2
fi
hub="$(antfrastructure_path linux/scripts/06-packaging)"
verify_locked_files="$(antfrastructure_path shared/python/verify_locked_files.py)"
data_lock_fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
# shellcheck source=lib/ort-runtime-bundle.sh
source "${script_dir}/lib/ort-runtime-bundle.sh"
# shellcheck source=/dev/null
source "${hub}/ort-package-proof.sh"
case "$3" in
    nvidia) kind=cuda; modules=(libonnxruntime_providers_shared.so libonnxruntime_providers_cuda.so) ;;
    amd) kind=migraphx; modules=("${ORT_AMDGPU_EP_MODULE}") ;;
    *) echo 'unsupported GPU vendor' >&2; exit 2 ;;
esac
# shellcheck source=/dev/null
source "$(antfrastructure_path linux/scripts/01-core/platform.sh)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# The ORT library loop below globs bundle files.
shopt -s nullglob
tarball="$(gpu_check_one_package "${package_dir}" .tar.gz)"
deb="$(gpu_check_one_package "${package_dir}" .deb)"
appimage="$(gpu_check_one_package "${package_dir}" .AppImage)"
deb_arch="$(arch_normalize "$arch")"
if [ "$(dpkg-deb --field "${deb}" Architecture)" != "$deb_arch" ] ||
   [ "$(dpkg-deb --field "${deb}" Package)" != oxidant ]; then
    echo 'GPU DEB package identity or architecture does not match its target' >&2
    exit 1
fi
case "$(dpkg-deb --field "${deb}" Version)" in
    *"+gpu-$3") ;;
    *) echo 'GPU DEB version does not match its vendor' >&2; exit 1 ;;
esac
mkdir "${work}/tar" "${work}/deb"
tar -xzf "${tarball}" -C "${work}/tar"
dpkg-deb -x "${deb}" "${work}/deb"
for bundle in "${work}/tar/oxidant" "${work}/deb/opt/oxidant"; do
    ort_assert_no_bundled_host_runtime "$bundle" "$kind"
    libraries=()
    ort_package_collect_libraries --reject-invalid --reject-links "$bundle" "${bundle}/lib" libraries \
        libonnxruntime.so libonnxruntime.so.1 "${modules[@]}"
    if [ "$kind" = migraphx ]; then
        # The plugin is no wheel's: the image's module and its tree of texts are the proof.
        ort_assert_plugin_module "${bundle}/lib/${ORT_AMDGPU_EP_MODULE}"
        verify_amdgpu_ep_notices "$bundle"
        if compgen -G "${bundle}/lib/libonnxruntime_providers_*" >/dev/null; then
            echo 'An AMD package carries an in-tree ONNX Runtime provider' >&2
            exit 1
        fi
    fi
    proven="$(mktemp -d "${work}/proof.XXXXXX")"
    # The NVIDIA prefix holds the wheel's own bytes; the CPU core an AMD package carries is the wheel's library stripped.
    exact=(--require-unstripped)
    [ "$kind" != migraphx ] || exact=()
    python3 "${hub}/stage-ort-wheel-licenses.py" "${exact[@]}" \
        --strip "${STRIP:-strip}" \
        --wheel-dir "${ORT_CHAIN_WHEEL_DIR:-/opt/onnxruntime-wheels}" --destination "$proven" "${libraries[@]}"
    ort_package_verify_notices "$bundle" "${bundle}/licenses/onnxruntime" "${proven}/licenses/onnxruntime"
done
bash "${hub}/verify-ort-appimage-payload.sh" "${appimage}" "$arch" \
    usr/lib/oxidant/lib usr/lib/oxidant/licenses/onnxruntime "$3"
image="$(realpath -e "${appimage}")"
mkdir "${work}/appimage"
(cd "${work}/appimage" && env -u LD_PRELOAD -u LD_AUDIT "$image" --appimage-extract 'usr/lib/oxidant/resources/models/*' >/dev/null)
if [ "$kind" = migraphx ]; then
    (cd "${work}/appimage" && env -u LD_PRELOAD -u LD_AUDIT "$image" --appimage-extract "usr/lib/oxidant/${ORT_AMDGPU_EP_NOTICES}/*" >/dev/null)
    verify_amdgpu_ep_notices "${work}/appimage/squashfs-root/usr/lib/oxidant"
fi
model_lock="${KATAGLYPHIS_REPO_ROOT}/resources/models/models.lock"
locked_files="$(models_lock_files "$model_lock")"
model_file_args=()
while IFS= read -r name; do model_file_args+=(--file "$name"); done <<< "$locked_files"
verify_model() {
    local root="$1" bundle="$2"
    python3 -I "$verify_locked_files" --root "$root" --directory "$bundle/resources/models" \
        --fetcher "$data_lock_fetcher" --lock "$model_lock" "${model_file_args[@]}"
}
verify_model "${work}/tar" "${work}/tar/oxidant"
verify_model "${work}/deb" "${work}/deb/opt/oxidant"
verify_model "${work}/appimage/squashfs-root" "${work}/appimage/squashfs-root/usr/lib/oxidant"
printf 'Finished %s package payloads match the chain wheel and locked model and exclude host runtimes\n' "$3"
