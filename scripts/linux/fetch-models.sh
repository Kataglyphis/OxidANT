#!/usr/bin/env bash
# Fetches each file models.lock (--llm: llm.lock, --scenes: scenes.lock, both unpacked) names into --dest (default: beside the lock), checked by SHA-256, through a cache: the hub's fetcher.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${root}/scripts/linux/lib/antfrastructure.sh"
fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
lock="${root}/resources/models/models.lock"
extract=()
# Packaging calls this for what a bundle carries, so the language models and the scenes come only on request.
case "${1-}" in
    --llm)
        shift
        lock="${root}/resources/models/llm.lock"
        extract=(--extract)
        ;;
    --scenes)
        shift
        lock="${root}/resources/scenes/scenes.lock"
        extract=(--extract)
        ;;
esac
exec python3 -I "${fetcher}" --lock "${lock}" "${extract[@]}" "$@"
