#!/usr/bin/env bash
# Fetches each file models.lock (--llm: llm.lock, unpacked) names into --dest (default: beside the lock), checked by SHA-256, through a cache: the hub's fetcher.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${root}/scripts/linux/lib/antfrastructure.sh"
fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
lock="${root}/resources/models/models.lock"
extract=()
# Packaging calls this for what a bundle carries, so the language models come only on request.
if [ "${1-}" = --llm ]; then
    shift
    lock="${root}/resources/models/llm.lock"
    extract=(--extract)
fi
exec python3 -I "${fetcher}" --lock "${lock}" "${extract[@]}" "$@"
