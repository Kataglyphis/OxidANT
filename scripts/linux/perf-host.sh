#!/usr/bin/env bash
# quality-report.sh --perf-host inside :latest: `setup` builds the release CLI, `text` is one crates/text run; it times no build.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"
CLI="target/release/kataglyphis_cli"
LLM_MODEL="resources/models/smollm2-perf"
PROMPT="Tell a short story about two cats on a sofa."
TOKENS=48

setup() {
  # shellcheck source=/dev/null
  source "${REPO_ROOT}/scripts/linux/lib/antfrastructure.sh"
  local fetcher
  fetcher="$(antfrastructure_path linux/scripts/lib/fetch-data-lock.py)"
  python3 -I "${fetcher}" --lock resources/models/models.lock
  python3 -I "${fetcher}" --lock resources/models/llm.lock --extract
  python3 -I "$(antfrastructure_path linux/scripts/lib/genai-model-threads.py)" \
    --model resources/models/smollm2_135m_instruct-cpu --out "${LLM_MODEL}" --threads 7
  cargo build --locked --release -p kataglyphis_cli
}

# The CLI prints its figures to stderr as `<n> tokens, <ms> ms to the first, <rate> tokens/s, finish <reason>`.
text() {
  local figures
  figures="$("${CLI}" chat --model-dir "${LLM_MODEL}" --prompt "${PROMPT}" --max-new-tokens "${TOKENS}" 2>&1 >/dev/null)"
  printf '%s\n' "${figures}" | sed -nE 's/^[0-9]+ tokens, ([0-9.]+) ms to the first, ([0-9.]+) tokens\/s, .*$/{"ttft_ms": \1, "decode_tok_s": \2}/p' | tail -1
}

case "${1:-}" in
  setup) setup ;;
  text) text ;;
  *) echo "usage: perf-host.sh setup|text" >&2; exit 2 ;;
esac
