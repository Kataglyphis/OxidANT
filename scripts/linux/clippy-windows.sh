#!/usr/bin/env bash
# Lints a crate for a Windows target from Linux, as the Windows lanes do: in :latest, as root.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
package=kataglyphis_observant
target=x86_64-pc-windows-gnu

usage() {
  printf 'usage: %s [--package NAME]\n' "${BASH_SOURCE[0]##*/}"
  printf 'clippy --all-targets -D warnings for %s: what the Windows lanes fail on an hour later.\n' "${target}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --package) package="${2:?--package needs a crate name}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done

# ring compiles C for the target; nothing else here needs a Windows toolchain.
if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  apt-get update -qq
  apt-get install -y -qq gcc-mingw-w64-x86-64 >/dev/null
fi
rustup target add "${target}" >/dev/null
cd "${root}"
# The host's .pc files stand in for the target's: clippy compiles, it links nothing.
PKG_CONFIG_ALLOW_CROSS=1 cargo clippy -p "${package}" --all-targets --locked --target "${target}" -- -D warnings
