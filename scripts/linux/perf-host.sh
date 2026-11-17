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
    --model resources/models/smollm2_135m_instruct-cpu --out "${LLM_MODEL}" --threads text
  cargo build --locked --release -p kataglyphis_cli
}

# A fresh process's first reply is noisy, so one run is the per-metric median of five; the CLI prints `<n> tokens, <ms> ms to the first, <rate> tokens/s, finish <reason>` to stderr.
text() {
  local figures ttft rate
  figures="$(for _ in 1 2 3 4 5; do
    "${CLI}" chat --model-dir "${LLM_MODEL}" --prompt "${PROMPT}" --max-new-tokens "${TOKENS}" 2>&1 >/dev/null |
      sed -nE 's/^[0-9]+ tokens, ([0-9.]+) ms to the first, ([0-9.]+) tokens\/s, .*$/\1 \2/p' | tail -1
  done)"
  ttft="$(cut -d' ' -f1 <<< "${figures}" | sort -g | sed -n 3p)"
  rate="$(cut -d' ' -f2 <<< "${figures}" | sort -g | sed -n 3p)"
  printf '{"ttft_ms": %s, "decode_tok_s": %s}\n' "${ttft}" "${rate}"
}

# A five-turn chat in one process, the median of three: turn 5's first token stays near turn 1's while the generator continues.
text_turns() {
  local figures
  figures="$(for _ in 1 2 3; do
    "${CLI}" chat --model-dir "${LLM_MODEL}" --prompt "${PROMPT}" --max-new-tokens "${TOKENS}" \
      --then "Give the grey cat a name." --then "What does the black cat do when the grey one sleeps?" \
      --then "Where do they go when it rains?" --then "End the story in one sentence." 2>&1 >/dev/null |
      sed -nE 's/^[0-9]+ tokens, ([0-9.]+) ms to the first, .*$/\1/p' | sed -n '1p;5p' | paste -sd' ' -
  done)"
  printf '{"turn1_ttft_ms": %s, "turn5_ttft_ms": %s}\n' \
    "$(cut -d' ' -f1 <<< "${figures}" | sort -g | sed -n 2p)" "$(cut -d' ' -f2 <<< "${figures}" | sort -g | sed -n 2p)"
}

case "${1:-}" in
  setup) setup ;;
  text) text ;;
  text-turns) text_turns ;;
  *) echo "usage: perf-host.sh setup|text|text-turns" >&2; exit 2 ;;
esac
