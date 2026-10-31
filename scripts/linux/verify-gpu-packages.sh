#!/usr/bin/env bash
# Check the finished Linux GPU packages before uploading a candidate.
set -euo pipefail
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${script_dir}/lib/antfrastructure.sh"
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
# shellcheck source=/dev/null
source "${hub}/ort-runtime-bundle.sh"
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
printf 'Finished %s package payloads match the GPU wheel and exclude host runtimes\n' "$3"
