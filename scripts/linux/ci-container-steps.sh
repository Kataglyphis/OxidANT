#!/usr/bin/env bash
# In-container half of reusable-linux.yml: the workflow names the step, this delegates to the hub driver.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

RUST_DRIVERS='linux/scripts/02-toolchain/rust'
# capture_test.rs is cfg(gstreamer); `?/` names the member's feature without enabling gui's and oxidant's optional deps.
TEST_FEATURES='kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'

usage() {
    cat >&2 <<'USAGE'
usage: ci-container-steps.sh <step>

  debug       cargo_debug.sh            - dev profile build
  security    cargo_security_checks.sh  - cargo audit + cargo deny (GATING)
  fmt-clippy  cargo_fmt_clippy.sh         - fmt --check + clippy -D warnings (GATING)
  test        cargo_test.sh             - unit + integration + proptest + doc
  coverage    cargo_coverage.sh         - tarpaulin over the workspace
  bench       cargo_bench.sh
  release     cargo_release.sh          - fat LTO release build
  docs        cargo_build_doc.sh        - rustdoc into target/doc
  ort-chain-only  check-ort-chain-only.sh - no downloaded ONNX Runtime (GATING)
  riscv64-test    the workspace tests, cross-built for riscv64 and run under QEMU

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
        antfrastructure_exec "${RUST_DRIVERS}/cargo_test.sh" --features "${TEST_FEATURES}" "$@"
        ;;
    # Without --workspace tarpaulin measures the root package alone, which has no unit tests: 0.00%.
    coverage)
        export KATAGLYPHIS_REQUIRE_GPU="${KATAGLYPHIS_REQUIRE_GPU-1}"
        antfrastructure_exec "${RUST_DRIVERS}/cargo_coverage.sh" --workspace --features "${TEST_FEATURES}" "$@"
        ;;
    bench)     antfrastructure_exec "${RUST_DRIVERS}/cargo_bench.sh" "$@" ;;
    release)   antfrastructure_exec "${RUST_DRIVERS}/cargo_release.sh" "$@" ;;
    docs)      antfrastructure_exec "${RUST_DRIVERS}/cargo_build_doc.sh" "$@" ;;
    # This repo's own gate, not a hub driver: the owner rule is chain-built ORT only.
    ort-chain-only) exec bash "${SCRIPT_DIR}/check-ort-chain-only.sh" "$@" ;;

    # Not --all-features: gui_unix needs GTK4, which the image lacks; exported since the helper execs.
    fmt-clippy)
        export CARGO_CLIPPY_ARGS="${CARGO_CLIPPY_ARGS---workspace --locked --features ${TEST_FEATURES}}"
        antfrastructure_exec "${RUST_DRIVERS}/cargo_fmt_clippy.sh" "$@"
        ;;

    # GPU suites skip unless RISCV64_GPU_TESTS=1: lavapipe under QEMU is the long pole. See AGENTS.md § The riscv64 lane
    riscv64-test)
        antfrastructure_source linux/scripts/lib/riscv64-cross.sh
        riscv64_cross_env
        if [ "${RISCV64_GPU_TESTS:-0}" = 1 ]; then
            export KATAGLYPHIS_REQUIRE_GPU=1
        else
            unset KATAGLYPHIS_REQUIRE_GPU
            export VK_LOADER_DRIVERS_DISABLE='*'
            echo "riscv64-test: GPU suites excluded (no Vulkan driver); RISCV64_GPU_TESTS=1 runs them on lavapipe"
        fi
        exec cargo test --workspace --locked --target riscv64gc-unknown-linux-gnu "$@"
        ;;

    -h|--help|help) usage; exit 0 ;;
    *)
        echo "ci-container-steps.sh: unknown step '${step}'" >&2
        usage
        exit 2
        ;;
esac
