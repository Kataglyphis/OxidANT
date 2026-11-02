#!/usr/bin/env bash
# Check the finished Linux GPU packages before uploading a candidate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${script_dir}/lib/antfrastructure.sh"
# shellcheck source=lib/models-lock.sh
source "${script_dir}/lib/models-lock.sh"
if [ "$#" -ne 3 ]; then
    printf 'Usage: %s PACKAGE_DIR x64|arm64 nvidia|amd\n' "$0" >&2
    exit 2
fi
package_dir="$(realpath -e "$1")"
case "$2" in x64|arm64) arch="$2" ;; *) echo 'unsupported package architecture' >&2; exit 2 ;; esac
case "$3" in
    nvidia) kind=cuda ;;
    amd) kind=migraphx ;;
    *) echo 'unsupported GPU vendor' >&2; exit 2 ;;
esac
if [ "$3" = amd ] && [ "$arch" = arm64 ]; then
    echo 'Linux arm64 ROCm packages are not supported' >&2
    exit 2
fi
hub="$(antfrastructure_path linux/scripts/06-packaging)"
verify_locked_files="$(antfrastructure_path shared/python/verify_locked_files.py)"
data_lock_fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
# shellcheck source=/dev/null
source "${hub}/ort-runtime-bundle.sh"
# shellcheck source=/dev/null
source "$(antfrastructure_path linux/scripts/01-core/platform.sh)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
shopt -s nullglob
tarballs=("${package_dir}"/*.tar.gz)
debs=("${package_dir}"/*.deb)
images=("${package_dir}"/*.AppImage)
if [ "${#tarballs[@]}" -ne 1 ] || [ "${#debs[@]}" -ne 1 ] || [ "${#images[@]}" -ne 1 ]; then
    echo 'Expected exactly one tarball, DEB and AppImage' >&2
    exit 1
fi
deb_arch="$(arch_normalize "$arch")"
if [ "$(dpkg-deb --field "${debs[0]}" Architecture)" != "$deb_arch" ] ||
   [ "$(dpkg-deb --field "${debs[0]}" Package)" != oxidant ]; then
    echo 'GPU DEB package identity or architecture does not match its target' >&2
    exit 1
fi
case "$(dpkg-deb --field "${debs[0]}" Version)" in
    *"+gpu-$3") ;;
    *) echo 'GPU DEB version does not match its vendor' >&2; exit 1 ;;
esac
mkdir "${work}/tar" "${work}/deb"
tar -xzf "${tarballs[0]}" -C "${work}/tar"
dpkg-deb -x "${debs[0]}" "${work}/deb"
for bundle in "${work}/tar/oxidant" "${work}/deb/opt/oxidant"; do
    ort_assert_no_bundled_host_runtime "$bundle" "$kind"
    for name in libonnxruntime.so libonnxruntime.so.1 libonnxruntime_providers_shared.so "libonnxruntime_providers_${kind}.so"; do
        file="${bundle}/lib/${name}"
        [ -f "$file" ] && [ ! -L "$file" ] || { printf 'Missing or linked ORT library: %s\n' "$file" >&2; exit 1; }
    done
    libraries=()
    for file in "${bundle}"/lib/libonnxruntime*.so*; do
        [ -f "$file" ] && [ ! -L "$file" ] || { printf 'Invalid or linked ORT library: %s\n' "$file" >&2; exit 1; }
        libraries+=(--library "$file")
    done
    proven="$(mktemp -d "${work}/proof.XXXXXX")"
    python3 "${hub}/stage-ort-wheel-licenses.py" --require-unstripped \
        --strip "${STRIP:-strip}" \
        --wheel-dir "${ORT_CHAIN_WHEEL_DIR:-/opt/onnxruntime-wheels}" --destination "$proven" "${libraries[@]}"
    for name in LICENSE ThirdPartyNotices.txt; do
        file="${bundle}/licenses/onnxruntime/${name}"
        [ -f "$file" ] && [ ! -L "$file" ] || { printf 'Missing or linked ORT notice: %s\n' "$file" >&2; exit 1; }
        cmp "$file" "${proven}/licenses/onnxruntime/${name}"
    done
done
bash "${hub}/verify-ort-appimage-payload.sh" "${images[0]}" "$arch" \
    usr/lib/oxidant/lib usr/lib/oxidant/licenses/onnxruntime "$3"
image="$(realpath -e "${images[0]}")"
mkdir "${work}/appimage"
(cd "${work}/appimage" && env -u LD_PRELOAD -u LD_AUDIT "$image" --appimage-extract 'usr/lib/oxidant/resources/models/*' >/dev/null)
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
printf 'Finished %s package payloads match the GPU wheel and locked model and exclude host runtimes\n' "$3"
