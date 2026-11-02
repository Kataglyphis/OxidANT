#!/usr/bin/env bash
# Profile every finished Linux GPU package without installing on the runner.
set -euo pipefail
# shellcheck source=lib/gpu-check-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/gpu-check-common.sh"
gpu_check_argc "$#" 2 3 'usage: profile-gpu-packages.sh PACKAGE_DIR cuda|migraphx [FIXTURE.jpg]'
package_dir="$(realpath -e "$1")" device="$2"
fixture="${3:-}"
if [ -n "${fixture}" ]; then fixture="$(realpath -e "${fixture}")"; fi
gpu_check_require_device "${device}"
command -v dpkg-deb >/dev/null || { echo 'dpkg-deb is required to inspect the DEB' >&2; exit 1; }
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tarball="$(gpu_check_one_package "${package_dir}" .tar.gz)"
deb="$(gpu_check_one_package "${package_dir}" .deb)"
appimage="$(gpu_check_one_package "${package_dir}" .AppImage)"
work="$(gpu_check_workspace gpu-packages)"
trap 'rm -rf -- "${work}"' EXIT
mkdir "${work}/tar" "${work}/deb" "${work}/appimage" "${work}/appimage-tmp"
tar -xzf "${tarball}" -C "${work}/tar"
echo 'Profiling the TAR launcher'
bash "${script_dir}/profile-gpu-bundle.sh" "${work}/tar/oxidant" "${device}" "${work}/tar-profile"
if [ -n "${fixture}" ]; then bash "${script_dir}/check-gpu-detector-parity.sh" "${work}/tar/oxidant/oxidant" "${device}" "${fixture}"; fi
rm -rf -- "${work}/tar"
dpkg-deb -x "${deb}" "${work}/deb"
echo 'Profiling the DEB payload launcher'
bash "${script_dir}/profile-gpu-bundle.sh" "${work}/deb/opt/oxidant" "${device}" "${work}/deb-profile"
if [ -n "${fixture}" ]; then bash "${script_dir}/check-gpu-detector-parity.sh" "${work}/deb/opt/oxidant/oxidant" "${device}" "${fixture}"; fi
rm -rf -- "${work}/deb"
# Actions drops the executable bit, so copy the AppImage into the private workspace.
image="${work}/candidate.AppImage"
cp -- "${appimage}" "${image}"
chmod u+x "${image}"
(cd "${work}/appimage" && env -u LD_PRELOAD -u LD_AUDIT -u APPIMAGE_EXTRACT_AND_RUN "${image}" --appimage-extract >/dev/null)
echo 'Profiling the finished AppImage launcher'
APPIMAGE_EXTRACT_AND_RUN=1 TMPDIR="${work}/appimage-tmp" \
    bash "${script_dir}/profile-gpu-bundle.sh" "${work}/appimage/squashfs-root/usr/lib/oxidant" \
    "${device}" "${work}/appimage-profile" "${image}"
if [ -n "${fixture}" ]; then
    APPIMAGE_EXTRACT_AND_RUN=1 TMPDIR="${work}/appimage-tmp" \
        bash "${script_dir}/check-gpu-detector-parity.sh" "${image}" "${device}" "${fixture}"
fi
echo 'All finished package launchers profiled the requested GPU provider'
