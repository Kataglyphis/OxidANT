#!/usr/bin/env bash
# collect-release-assets.sh <artifacts dir> <out dir> <version>: the lanes' packages, flat and uniquely named, for a GitHub release.
set -euo pipefail

artifacts="${1:?usage: collect-release-assets.sh <artifacts dir> <out dir> <version>}"
out="${2:?usage: collect-release-assets.sh <artifacts dir> <out dir> <version>}"
version="${3:?usage: collect-release-assets.sh <artifacts dir> <out dir> <version>}"
mkdir -p "$out"
# Absolute, since the zip below runs inside each bundle folder; release.yml passes a relative one.
out="$(cd "$out" && pwd)"

# put <file> <name>: one release file; a second one of the same name would silently replace the first.
put() {
  if [ -e "${out}/$2" ]; then
    printf 'two artifacts carry %s\n' "$2" >&2
    exit 1
  fi
  cp "$1" "${out}/$2"
}

for dir in "$artifacts"/*/; do
  name="$(basename "$dir")"
  # Test bundles and results are no deliverable.
  case "$name" in *-tests|*-test-results) continue ;; esac
  while IFS= read -r -d '' file; do
    put "$file" "$(basename "$file")"
  done < <(find "$dir" -type f \( -name '*.tar.gz' -o -name '*.deb' -o -name '*.AppImage' -o -name '*.msi' -o -name '*.msix' \) -print0)
  # package_archive.sh names a flatpak without its arch, so both arches' would collide.
  for flatpak in "$dir"dist/*.flatpak; do
    [ -e "$flatpak" ] || continue
    case "$name" in *-arm64-all) arch=aarch64 ;; *-x64-all) arch=x86_64 ;; *) printf 'no arch in %s\n' "$name" >&2; exit 1 ;; esac
    put "$flatpak" "$(basename "${flatpak%.flatpak}")-${arch}.flatpak"
  done
  # The portable Windows build is a folder; a release carries it zipped.
  if [ -d "${dir}bundle" ]; then
    zip_name="${name%-windows-*}-${version}-windows-${name##*-windows-}.zip"
    [ ! -e "${out}/${zip_name}" ] || { printf 'two artifacts carry %s\n' "$zip_name" >&2; exit 1; }
    (cd "${dir}bundle" && zip -qr "${out}/${zip_name}" .)
  fi
done

if [ -z "$(ls -A "$out")" ]; then
  printf 'no packages under %s\n' "$artifacts" >&2
  exit 1
fi
ls -1 "$out"
