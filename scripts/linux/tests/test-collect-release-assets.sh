#!/usr/bin/env bash
# test-collect-release-assets.sh: collect-release-assets.sh against artifact trees shaped like the lanes' uploads.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
collect="${here}/../collect-release-assets.sh"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
fails=0
check() {
  if "${@:2}"; then printf 'ok    %s\n' "$1"; else printf 'FAIL  %s\n' "$1"; fails=$((fails + 1)); fi
}

a="${work}/artifacts"
for arch in x64 arm64; do
  d="${a}/oxidant-2.3.4-ubuntu-26.04-${arch}-all"
  mkdir -p "${d}/dist" "${d}/target/release"
  printf 'tar' > "${d}/dist/oxidant-2.3.4-linux_${arch}.tar.gz"
  printf 'deb' > "${d}/dist/oxidant_2.3.4_${arch}.deb"
  printf 'img' > "${d}/dist/oxidant-2.3.4-${arch}.AppImage"
  printf 'flatpak' > "${d}/dist/oxidant-2.3.4.flatpak"
  printf 'path' > "${d}/dist/archive_path.txt"
  printf 'elf' > "${d}/target/release/kataglyphis_cli"
  printf 'report' > "${d}/tarpaulin-report.html"
done
w="${a}/kataglyphis_cli-windows-x64"
mkdir -p "${w}/msix" "${w}/msi" "${w}/bundle/lib"
printf 'msix' > "${w}/msix/Kataglyphis.OxidANT_2.3.4.0_x64.msix"
printf 'msi' > "${w}/msi/kataglyphis_cli-2.3.4-x64.msi"
printf 'pdb' > "${w}/msi/kataglyphis_cli-2.3.4-x64.wixpdb"
printf 'exe' > "${w}/bundle/kataglyphis_cli.exe"
printf 'dll' > "${w}/bundle/lib/gst.dll"
mkdir -p "${a}/kataglyphis_cli-windows-arm64/bundle"
printf 'exe' > "${a}/kataglyphis_cli-windows-arm64/bundle/kataglyphis_cli.exe"
mkdir -p "${a}/kataglyphis_cli-windows-arm64-tests/msi"
printf 'msi' > "${a}/kataglyphis_cli-windows-arm64-tests/msi/tests.msi"

out="${work}/release"
check "it collects" bash "${collect}" "${a}" "${out}" 2.3.4
check "every Linux package, both arches" test "$(find "${out}" -maxdepth 1 -type f \( -name '*.tar.gz' -o -name '*.deb' -o -name '*.AppImage' \) | wc -l)" = 6
check "each flatpak named for its arch" test -e "${out}/oxidant-2.3.4-x86_64.flatpak" -a -e "${out}/oxidant-2.3.4-aarch64.flatpak"
check "the MSI and the MSIX" test -e "${out}/kataglyphis_cli-2.3.4-x64.msi" -a -e "${out}/Kataglyphis.OxidANT_2.3.4.0_x64.msix"
check "the portable Windows folder, zipped with its subfolders" bash -c "unzip -l '${out}/kataglyphis_cli-2.3.4-windows-x64.zip' | grep -q 'lib/gst.dll'"
check "no reports, loose binaries, wix symbols or path notes" bash -c "! ls '${out}' | grep -qE 'tarpaulin|archive_path|wixpdb|^kataglyphis_cli$'"
check "nothing from a test artifact" test ! -e "${out}/tests.msi"
# Relative paths, as release.yml passes them: the first dry run died zipping into /.
check "relative paths, as release.yml passes them" bash -c "cd '${work}' && bash '${collect}' artifacts release-relative 2.3.4 >/dev/null"
check "both Windows zips land in the relative out dir" test -e "${work}/release-relative/kataglyphis_cli-2.3.4-windows-x64.zip" -a -e "${work}/release-relative/kataglyphis_cli-2.3.4-windows-arm64.zip"

mkdir -p "${a}/kataglyphis_cli-windows-x64-copy/msi"
cp "${w}/msi/kataglyphis_cli-2.3.4-x64.msi" "${a}/kataglyphis_cli-windows-x64-copy/msi/"
check "two artifacts with one file name fail rather than overwrite" bash -c "! bash '${collect}' '${a}' '${work}/release2' 2.3.4 >/dev/null 2>&1"
mkdir -p "${work}/none/x"
check "no packages at all fail" bash -c "! bash '${collect}' '${work}/none' '${work}/release3' 2.3.4 >/dev/null 2>&1"

if [ "${fails}" -gt 0 ]; then
  printf '%d of the collect checks failed\n' "${fails}"
  exit 1
fi
printf 'collect-release-assets: all checks passed\n'
