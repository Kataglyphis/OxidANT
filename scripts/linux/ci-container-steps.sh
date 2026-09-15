#!/usr/bin/env bash
# ci-container-steps.sh - the in-container half of .github/workflows/rust_ubuntu26_04.yml.
#
# ONE NAME PER CI STEP, so the workflow says WHICH step runs and this file says
# HOW. Before it, every step in that workflow carried its own copy of
#
#   bash -lc 'set -e; git config --global --add safe.directory /workspace || true;
#             bash third_party/ANTfrastructure/linux/scripts/02-toolchain/rust/cargo_<x>.sh'
#
# eight times over, which is eight places to edit when the prologue changes and
# eight chances for one of them to drift. It also made the lane impossible to
# reproduce by hand without retyping a container command out of YAML; now it is
#
#   bash scripts/linux/ci-container-steps.sh <step>
#
# inside the family Linux image, and the workflow runs exactly that.
#
# THE DRIVERS ARE ANTfrastructure'S, NOT COPIES. Every step below delegates to
# linux/scripts/02-toolchain/rust/cargo_*.sh through the wrapper in lib/, which
# is the repo-wide rule (see AGENTS.md, "ANTfrastructure is the ground truth").
# There is no longer an exception: `fmt-clippy` hand-rolled the two cargo calls
# until the driver grew CARGO_CLIPPY_ARGS, and it delegates like the rest now.
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

Run inside the family Linux CI image, from the repository root.
USAGE
}

# The container runs as uid 1001 while the bind-mounted checkout is owned by
# the runner's uid, so git refuses the tree as "dubious ownership" and every
# driver that shells out to git (version stamping, rustdoc's source links)
# fails in a way that names neither ownership nor the mount. Kept here rather
# than in each workflow step; ANTfrastructure's own cargo wrappers are getting
# the same guard (CARGO_SAFE_DIRECTORY), and when they have it this drops out.
: "${CARGO_SAFE_DIRECTORY:=${KATAGLYPHIS_REPO_ROOT}}"
git config --global --add safe.directory "${CARGO_SAFE_DIRECTORY}" || true

step="${1-}"
[ "$#" -ge 1 ] || { usage; exit 2; }
shift

case "$step" in
    debug)     antfrastructure_exec "${RUST_DRIVERS}/cargo_debug.sh" "$@" ;;
    security)  antfrastructure_exec "${RUST_DRIVERS}/cargo_security_checks.sh" "$@" ;;
    test)      antfrastructure_exec "${RUST_DRIVERS}/cargo_test.sh" "$@" ;;
    coverage)  antfrastructure_exec "${RUST_DRIVERS}/cargo_coverage.sh" "$@" ;;
    bench)     antfrastructure_exec "${RUST_DRIVERS}/cargo_bench.sh" "$@" ;;
    release)   antfrastructure_exec "${RUST_DRIVERS}/cargo_release.sh" "$@" ;;
    docs)      antfrastructure_exec "${RUST_DRIVERS}/cargo_build_doc.sh" "$@" ;;

    # CARGO_CLIPPY_ARGS is why this case can delegate at all. The driver used
    # to hard-code `cargo clippy --all-targets --all-features`, and this image
    # cannot build --all-features: gui_unix needs GTK4 headers, the onnxruntime
    # features need vendor SDKs, and uid 1001 cannot apt-get install either. So
    # the two calls were written out here instead. Since the pinned hub the
    # scope is a knob, and these are the values the hand-rolled pair used:
    # `cargo clippy --all-targets --workspace --locked -- -D warnings`.
    #
    # Overridable from the environment for a one-off (CARGO_CLIPPY_ARGS=''
    # means clippy's own defaults), and exported rather than prefixed because
    # antfrastructure_exec ends in `exec`.
    #
    # `"$@"` now reaches `cargo fmt` only. The driver stopped forwarding one
    # argument list to two tools that read it differently - a --features meant
    # for fmt used to become a scope change for clippy.
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
