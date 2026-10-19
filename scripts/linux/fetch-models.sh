#!/usr/bin/env bash
# Fetches each file models.lock names into --dest (default: beside the lock), checked by SHA-256, through a cache: the hub's fetcher.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${root}/scripts/linux/lib/antfrastructure.sh"
fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
exec python3 -I "${fetcher}" --lock "${root}/resources/models/models.lock" "$@"
