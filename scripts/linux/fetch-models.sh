#!/usr/bin/env bash
# Fetches each file models.lock names into --dest (default: beside the lock), checked by SHA-256, through a cache.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
lock="${root}/resources/models/models.lock"
dest=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dest) dest="${2:?--dest needs a directory}"; shift 2 ;;
    --lock) lock="${2:?--lock needs a file}"; shift 2 ;;
    -h|--help) printf 'usage: %s [--dest DIR] [--lock FILE]  (cache: $KATAGLYPHIS_MODEL_CACHE)\n' "$0"; exit 0 ;;
    *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
  esac
done
[[ -f "${lock}" ]] || { printf 'no lock file %s\n' "${lock}" >&2; exit 2; }
dest="${dest:-$(dirname "${lock}")}"
cache="${KATAGLYPHIS_MODEL_CACHE:-${XDG_CACHE_HOME:-${HOME}/.cache}/kataglyphis-models}"
mkdir -p "${dest}" "${cache}"

sha_of() {
  sha256sum "$1" | cut -d' ' -f1
  return 0
}

while read -r sha file url; do
  case "${sha}" in '' | '#'*) continue ;; esac
  target="${dest}/${file}"
  if [[ -f "${target}" && "$(sha_of "${target}")" == "${sha}" ]]; then
    continue
  fi
  cached="${cache}/${sha}"
  if [[ ! -f "${cached}" || "$(sha_of "${cached}")" != "${sha}" ]]; then
    part="$(mktemp "${cache}/.part.XXXXXX")"
    if ! curl -fsSL --retry 3 -o "${part}" "${url}"; then
      rm -f "${part}"
      printf 'download failed: %s\n' "${url}" >&2
      exit 1
    fi
    got="$(sha_of "${part}")"
    if [[ "${got}" != "${sha}" ]]; then
      rm -f "${part}"
      printf '%s: sha256 %s, the lock says %s\n' "${url}" "${got}" "${sha}" >&2
      exit 1
    fi
    mv "${part}" "${cached}"
  fi
  cp "${cached}" "${target}.part"
  # mktemp's 0600 would follow the copy, and a service user must read the model.
  chmod 644 "${target}.part"
  mv "${target}.part" "${target}"
  printf 'fetched %s (sha256 %s)\n' "${target}" "${sha:0:12}"
done <"${lock}"
