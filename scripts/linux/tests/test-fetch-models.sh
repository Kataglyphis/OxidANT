#!/usr/bin/env bash
# fetch-models.sh hands its lock to the hub's fetcher: the default fetches files, --llm and --scenes unpack archives.
set -u
tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${tests_dir}/../../.." && pwd)"
source "${repo}/scripts/linux/lib/antfrastructure.sh"
# shellcheck source=third_party/ANTfrastructure/linux/scripts/tests/test-harness.sh
source "$(antfrastructure_path linux/scripts/tests/test-harness.sh)"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
mkdir -p "${work}/hub/linux/scripts/lib"
printf 'import sys\nprint(" ".join(sys.argv[1:]))\n' > "${work}/hub/linux/scripts/lib/fetch-data-lock.py"
fetch() { ANTFRASTRUCTURE_DIR="${work}/hub" bash "${repo}/scripts/linux/fetch-models.sh" "$@" 2>&1; }

t_case 'each flag names its lock, and the archives unpack'
t_assert_eq "--lock ${repo}/resources/models/models.lock --dest out" "$(fetch --dest out)" 'the models, as files'
t_assert_eq "--lock ${repo}/resources/models/llm.lock --extract" "$(fetch --llm)" 'the language models'
t_assert_eq "--lock ${repo}/resources/scenes/scenes.lock --extract --only Colosseum-web" \
    "$(fetch --scenes --only Colosseum-web)" 'the scenes, the rest of the line passed on'

t_case 'the scene lock pins both Colosseums by SHA-256, in the format the fetcher reads'
rows="$(grep -v '^#' "${repo}/resources/scenes/scenes.lock")"
t_assert_contains "${rows}" 'Colosseum-gltf-v1/Colosseum-gltf.tar.xz' 'the native scene'
t_assert_contains "${rows}" 'Colosseum-web-v1/Colosseum-web.tar.xz' 'the browser scene'
t_assert_eq 2 "$(grep -cE '^[0-9a-f]{64}  [^ ]+\.tar\.xz +https://' <<< "${rows}")" 'two well-formed rows'

t_summary
