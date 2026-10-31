#!/usr/bin/env bash
# Profile every finished Linux GPU package without installing on the runner.
set -euo pipefail
if [ "$#" -ne 2 ]; then
    echo 'usage: profile-gpu-packages.sh PACKAGE_DIR cuda|migraphx' >&2
    exit 2
fi
package_dir="$(realpath -e "$1")" device="$2"
case "${device}" in cuda|migraphx) ;; *) echo 'unsupported GPU device' >&2; exit 2 ;; esac
command -v dpkg-deb >/dev/null || { echo 'dpkg-deb is required to inspect the DEB' >&2; exit 1; }
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
shopt -s nullglob
tarballs=("${package_dir}"/*.tar.gz)
debs=("${package_dir}"/*.deb)
images=("${package_dir}"/*.AppImage)
if [ "${#tarballs[@]}" -ne 1 ] || [ "${#debs[@]}" -ne 1 ] || [ "${#images[@]}" -ne 1 ]; then
    echo 'Expected exactly one tarball, DEB and AppImage' >&2
    exit 1
fi
work="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/oxidant-gpu-packages.XXXXXX")"
trap 'rm -rf -- "${work}"' EXIT
mkdir "${work}/tar" "${work}/deb" "${work}/appimage" "${work}/appimage-tmp"
tar -xzf "${tarballs[0]}" -C "${work}/tar"
echo 'Profiling the TAR launcher'
bash "${script_dir}/profile-gpu-bundle.sh" "${work}/tar/oxidant" "${device}" "${work}/tar-profile"
rm -rf -- "${work}/tar"
dpkg-deb -x "${debs[0]}" "${work}/deb"
echo 'Profiling the DEB payload launcher'
bash "${script_dir}/profile-gpu-bundle.sh" "${work}/deb/opt/oxidant" "${device}" "${work}/deb-profile"
rm -rf -- "${work}/deb"
# Actions drops the executable bit, so copy the AppImage into the private workspace.
image="${work}/candidate.AppImage"
cp -- "${images[0]}" "${image}"
chmod u+x "${image}"
(cd "${work}/appimage" && env -u LD_PRELOAD -u LD_AUDIT -u APPIMAGE_EXTRACT_AND_RUN "${image}" --appimage-extract >/dev/null)
echo 'Profiling the finished AppImage launcher'
APPIMAGE_EXTRACT_AND_RUN=1 TMPDIR="${work}/appimage-tmp" \
    bash "${script_dir}/profile-gpu-bundle.sh" "${work}/appimage/squashfs-root/usr/lib/oxidant" \
    "${device}" "${work}/appimage-profile" "${image}"
echo 'All finished package launchers profiled the requested GPU provider'
