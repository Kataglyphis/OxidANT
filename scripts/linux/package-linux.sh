#!/usr/bin/env bash
# The CLI as a self-contained bundle (its GStreamer, ONNX Runtime, model and C library), packed as tar, deb, AppImage and flatpak.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

usage() {
    cat >&2 <<'USAGE'
usage: package-linux.sh --binary-path FILE --version V --arch x64|arm64 [options]

  --binary NAME          command the packages install (default: oxidant)
  --binary-path FILE     the release kataglyphis_cli
  --version V            package version
  --arch x64|arm64       package architecture
  --platform P           only names the tarball, e.g. linux/amd64
  --package-types CSV    tar,deb,appimage,flatpak (default: all four)
  --desktop-file FILE    desktop entry (default: packaging/linux/oxidant.desktop)
  --icon-file FILE       icon (default: images/logo.png)
  --out DIR              where the packages land (default: dist)

Run inside the family Linux CI image, from the repository root.
USAGE
}

binary=oxidant
binary_path=""
version=""
arch=""
platform=""
package_types="tar,deb,appimage,flatpak"
desktop_file="${REPO_ROOT}/packaging/linux/oxidant.desktop"
icon_file="${REPO_ROOT}/images/logo.png"
out_dir="${REPO_ROOT}/dist"
while [ $# -gt 0 ]; do
    case "$1" in
        --binary) binary="${2:?}"; shift 2 ;;
        --binary-path) binary_path="${2:?}"; shift 2 ;;
        --version) version="${2:?}"; shift 2 ;;
        --arch) arch="${2:?}"; shift 2 ;;
        --platform) platform="${2:?}"; shift 2 ;;
        --package-types) package_types="${2:?}"; shift 2 ;;
        --desktop-file) desktop_file="$(realpath "${2:?}")"; shift 2 ;;
        --icon-file) icon_file="$(realpath "${2:?}")"; shift 2 ;;
        --out) out_dir="${2:?}"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; usage; exit 2 ;;
    esac
done
[ -x "${binary_path}" ] || { printf 'no executable --binary-path: %s\n' "${binary_path:-<empty>}" >&2; exit 2; }
[ -n "${version}" ] || { printf -- '--version is required\n' >&2; exit 2; }
# The image's login shell exports ARCH as uname -m spells it, so the lanes' --arch "${ARCH}" arrives that way.
case "${arch}" in
    x64|x86_64|amd64) arch=x64 ;;
    arm64|aarch64) arch=arm64 ;;
    *) printf -- '--arch must be x64 or arm64, not %s\n' "${arch:-<empty>}" >&2; exit 2 ;;
esac
mkdir -p "${out_dir}"
out_dir="$(realpath "${out_dir}")"

hub() { antfrastructure_path "linux/scripts/06-packaging/$1"; }

# Container-native: the bind mount refuses the chmod and utime the packers need.
work="${KATAGLYPHIS_PACKAGING_WORKDIR:-/tmp/packaging-work}"
bundle="${work}/${binary}"
rm -rf "${bundle}"
mkdir -p "${bundle}/lib/gstreamer-1.0"
install -m 0755 "${binary_path}" "${bundle}/${binary}"

# The GUI's pipeline (crates/gui/src/gui_wgpu/pipeline.rs) and autovideosrc's camera and test sources.
plugins=(coreelements app videoconvertscale autodetect video4linux2 videotestsrc)
elements=(appsink autovideosrc capsfilter v4l2src videoconvert videotestsrc)
gst_copy=(copy --dest "${bundle}/lib/gstreamer-1.0" --plugin-dir "$(pkg-config --variable=pluginsdir gstreamer-1.0)")
gtk_plugin="$(gst-inspect-1.0 gtk4paintablesink | awk '$1 == "Filename" { print $2; exit }')"
[ -f "${gtk_plugin}" ] || { printf 'gtk4paintablesink is missing from this image\n' >&2; exit 1; }
gst_copy+=(--plugin-dir "$(dirname "${gtk_plugin}")")
plugins+=(gtk4)
elements+=(gtk4paintablesink)
for plugin in "${plugins[@]}"; do gst_copy+=(--plugin "${plugin}"); done
bash "$(hub gstreamer-bundle.sh)" "${gst_copy[@]}"

# The chain ONNX Runtime only (owner rule 2026-09-23), copied as is: G6 below proves the bytes.
ort_dir="${ORT_LIB_LOCATION:-/usr/local/lib/onnxruntime-cpu/lib}"
for soname in libonnxruntime.so libonnxruntime.so.1; do
    cp -L "${ort_dir}/${soname}" "${bundle}/lib/${soname}"
done

# The compiler's own runtime: the image also carries an older distro libstdc++.
cxx_dir() { dirname "$("${CXX:-g++}" -print-file-name="$1")"; }
closure=(--bundle "${bundle}" --lib-dir "${bundle}/lib" --relocatable --protect 'libonnxruntime*'
    --from "libonnxruntime*=${ort_dir}" --from "libstdc++.so.6=$(cxx_dir libstdc++.so.6)"
    --from "libgcc_s.so.1=$(cxx_dir libgcc_s.so.1)" --no-default-system)
# glibc stays the system's; bundle-loader.sh carries a copy for older hosts.
glibc=(libc.so.6 libm.so.6 libmvec.so.1 libpthread.so.0 libdl.so.2 librt.so.1 libresolv.so.2 libutil.so.1
    libanl.so.1 libnsl.so.1 'libnss_*.so.2' 'ld-linux*.so*')
for soname in "${glibc[@]}"; do
    closure+=(--system "${soname}")
done
# The image reaches these only through ld.so.conf, which no target has.
for dir in "$(pkg-config --variable=libdir gstreamer-1.0)" "$(pkg-config --variable=libdir gtk4)" /opt/gstreamer/lib /opt/ffmpeg/lib /opt/libcamera/lib; do
    closure+=(--search "${dir}")
done
bash "$(hub elf-closure.sh)" "${closure[@]}" --ldconfig
# Match the delivered ORT bytes before using the wheel's original notices.
# shellcheck source=third_party/ANTfrastructure/linux/scripts/06-packaging/ort-wheel-licenses.sh
source "$(antfrastructure_path linux/scripts/06-packaging/ort-wheel-licenses.sh)"
stage_ort_wheel_licenses "${bundle}"
bash "$(hub bundle-loader.sh)" --beside "${bundle}" "${bundle}/lib" "${binary}"
bash "${SCRIPT_DIR}/fetch-models.sh" --dest "${bundle}/resources/models"

# Proof before packing: every ELF resolves inside, ORT is the chain's, each element loads from the bundle.
bash "$(hub elf-closure.sh)" "${closure[@]}" --check
bash "$(hub check-ort-provenance.sh)" "${bundle}"
bash "$(hub gstreamer-bundle.sh)" check --plugins "${bundle}/lib/gstreamer-1.0" --lib-dir "${bundle}/lib" "${elements[@]}"
# A scrubbed environment, so nothing of the image's GStreamer or ORT setup reaches the run.
for command in --help onnx-runtime media-check; do
    env -i HOME="${work}/home" PATH=/usr/bin:/bin "${bundle}/${binary}" "${command}" >/dev/null
done
printf 'bundle %s: %s\n' "${bundle}" "$(du -sh "${bundle}" | cut -f1)"

tarball="${out_dir}/${binary}-${version//\//-}${platform:+-${platform/\//_}}-${arch}.tar.gz"
export APP_PACKAGING_BUNDLE_DIR="${bundle}"
export APP_PACKAGING_VERSION="${version#v}"
export APP_PACKAGING_DESKTOP_FILE="${desktop_file}"
export APP_PACKAGING_ICON_FILE="${icon_file}"
export APP_PACKAGING_OUT_DIR="${out_dir}"
# glibc travels and the launcher picks it on an older host, so the C library is all a deb needs.
export APP_PACKAGING_DEB_DEPENDS="${APP_PACKAGING_DEB_DEPENDS:-libc6}"
# The GUI's camera; flatpak has no --device=video.
export KATAGLYPHIS_FLATPAK_FINISH_ARGS="${KATAGLYPHIS_FLATPAK_FINISH_ARGS---device=all}"
# shellcheck source=/dev/null
source "$(antfrastructure_path linux/scripts/lib/app-packaging.sh)"
app_packaging_stage_project_license "${REPO_ROOT}/LICENSE" "${bundle}"
case "${arch}" in x64) rust_target=x86_64-unknown-linux-gnu ;; arm64) rust_target=aarch64-unknown-linux-gnu ;; esac
# The packaging container may not have extracted every crate the shipped CLI's target graph names.
cargo fetch --locked --manifest-path "${REPO_ROOT}/Cargo.toml" --target "${rust_target}"
python3 "$(hub stage-cargo-licenses.py)" --workspace "${REPO_ROOT}" --root-package kataglyphis_cli \
    --target "${rust_target}" --policy "${REPO_ROOT}/packaging/cargo-license-policy.json" --output-dir "${bundle}/licenses/cargo"
license_verifier="$(antfrastructure_path linux/scripts/verify_package_license.py)"
# Every staged crate text and model notice, read back from the finished package.
third_party_rules() {
    local prefix="$1" text
    rules=(--require "${prefix}/LICENSE=${REPO_ROOT}/LICENSE")
    while IFS= read -r -d '' text; do
        rules+=(--require "${prefix}/${text#"${bundle}/"}=${text}")
    done < <(find "${bundle}/licenses/cargo" "${bundle}/resources/models" -type f \
        \( -path '*/licenses/cargo/*' -o -name '*.LICENSE' -o -name '*.NOTICE' \) -print0 | sort -z)
}
package_name="$(app_packaging_sanitize_package_name "${binary}")"
read -r -a formats <<< "${package_types//,/ }"
for format in "${formats[@]}"; do
    case "${format}" in
        tar)
            tar -C "${work}" -czf "${tarball}" "${binary}"
            third_party_rules "${binary}"
            python3 "${license_verifier}" "${tarball}" "${rules[@]}"
            verify_ort_wheel_package_licenses "${tarball}" "${bundle}" "${binary}"
            ;;
        deb)
            app_packaging_package_linux_bundle_deb "${arch}" "${binary}"
            deb_arch="$(app_packaging_map_arch_to_deb "${arch}")"
            third_party_rules "opt/${package_name}"
            python3 "${license_verifier}" "${out_dir}/${package_name}_${version#v}_${deb_arch}.deb" "${rules[@]}"
            verify_ort_wheel_package_licenses "${out_dir}/${package_name}_${version#v}_${deb_arch}.deb" "${bundle}" "opt/${package_name}"
            ;;
        appimage)
            app_packaging_package_linux_bundle_appimage "${arch}" "${binary}"
            appimage_arch="$(app_packaging_map_arch_to_appimage "${arch}")"
            bash "$(antfrastructure_path linux/scripts/06-packaging/verify-native-appimage-license.sh)" \
                "${out_dir}/${package_name}-${version#v}-${appimage_arch}.AppImage" "${arch}" "usr/lib/${package_name}/LICENSE" "${REPO_ROOT}/LICENSE"
            # One extraction per entry, so the crate manifest stands in for the tree the archive formats check file by file.
            for text in licenses/cargo/manifest.json resources/models/deim_dfine_n_coco.LICENSE; do
                bash "$(antfrastructure_path linux/scripts/06-packaging/verify-native-appimage-license.sh)" \
                    "${out_dir}/${package_name}-${version#v}-${appimage_arch}.AppImage" "${arch}" "usr/lib/${package_name}/${text}" "${bundle}/${text}"
            done
            verify_ort_wheel_appimage_licenses "${out_dir}/${package_name}-${version#v}-${appimage_arch}.AppImage" "${bundle}" "${arch}" "usr/lib/${package_name}"
            ;;
        flatpak)
            app_packaging_package_linux_bundle_flatpak "${arch}" "${binary}"
            third_party_rules "files/lib/${package_name}"
            python3 "${license_verifier}" "${out_dir}/${package_name}-${version#v}.flatpak" "${rules[@]}"
            verify_ort_wheel_package_licenses "${out_dir}/${package_name}-${version#v}.flatpak" "${bundle}" "files/lib/${package_name}"
            ;;
        *) printf 'unknown package type: %s\n' "${format}" >&2; exit 2 ;;
    esac
done
ls -l "${out_dir}"
