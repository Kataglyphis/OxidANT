#!/usr/bin/env bash
# In-container half of reusable-linux.yml: the workflow names the step, this delegates to the hub driver.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

RUST_DRIVERS='linux/scripts/02-toolchain/rust'

usage() {
    cat >&2 <<'USAGE'
usage: ci-container-steps.sh <step>

  debug       cargo_debug.sh            - dev profile build
  security    cargo_security_checks.sh  - cargo audit + cargo deny (GATING)
  fmt-clippy  cargo_fmt_clippy.sh         - fmt --check + clippy -D warnings (GATING)
  test        cargo_test.sh             - unit + integration + proptest + doc
  coverage    cargo_coverage.sh         - tarpaulin
  bench       cargo_bench.sh
  release     cargo_release.sh          - fat LTO release build
  docs        cargo_build_doc.sh        - rustdoc into target/doc
  ort-chain-only  check-ort-chain-only.sh - no downloaded ONNX Runtime (GATING)

Run inside the family Linux CI image, from the repository root.
USAGE
}

# Uid 1001 does not own the bind mount, so git would refuse it as dubious ownership.
: "${CARGO_SAFE_DIRECTORY:=${KATAGLYPHIS_REPO_ROOT}}"
git config --global --add safe.directory "${CARGO_SAFE_DIRECTORY}" || true

step="${1-}"
[ "$#" -ge 1 ] || { usage; exit 2; }
shift

case "$step" in
    debug)     antfrastructure_exec "${RUST_DRIVERS}/cargo_debug.sh" "$@" ;;
    security)  antfrastructure_exec "${RUST_DRIVERS}/cargo_security_checks.sh" "$@" ;;
    # Golden tests must render (the image has lavapipe), not skip; `-` lets an empty value opt out.
    test)
        export KATAGLYPHIS_REQUIRE_GPU="${KATAGLYPHIS_REQUIRE_GPU-1}"
        antfrastructure_exec "${RUST_DRIVERS}/cargo_test.sh" "$@"
        ;;
    coverage)  antfrastructure_exec "${RUST_DRIVERS}/cargo_coverage.sh" "$@" ;;
    bench)     antfrastructure_exec "${RUST_DRIVERS}/cargo_bench.sh" "$@" ;;
    release)   antfrastructure_exec "${RUST_DRIVERS}/cargo_release.sh" "$@" ;;
    docs)      antfrastructure_exec "${RUST_DRIVERS}/cargo_build_doc.sh" "$@" ;;
    # This repo's own gate, not a hub driver: the owner rule is chain-built ORT only.
    ort-chain-only) exec bash "${SCRIPT_DIR}/check-ort-chain-only.sh" "$@" ;;

    # Not --all-features: gui_unix needs GTK4, which the image lacks; exported since the helper execs.
    fmt-clippy)
        export CARGO_CLIPPY_ARGS="${CARGO_CLIPPY_ARGS---workspace --locked}"
        antfrastructure_exec "${RUST_DRIVERS}/cargo_fmt_clippy.sh" "$@"
        ;;

    -h|--help|help) usage; exit 0 ;;
    *)
        echo "ci-container-steps.sh: unknown step '${step}'" >&2
        usage
        exit 2
        ;;
esac
