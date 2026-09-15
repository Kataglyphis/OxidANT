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
# THE DRIVERS ARE ANTfrastructure'S, NOT COPIES. Everything below delegates to
# linux/scripts/02-toolchain/rust/cargo_*.sh through the wrapper in lib/, which
# is the repo-wide rule (see AGENTS.md, "ANTfrastructure is the ground truth").
# The one exception is `fmt-clippy`, and the reason is written at that case.
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
  fmt-clippy  cargo fmt --check + cargo clippy -D warnings (GATING)
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

    fmt-clippy)
        # THE ONE STEP THAT DOES NOT USE THE DRIVER, and not for the reason the
        # workflow used to give. `cargo_fmt_clippy.sh` opened with
        # `rustup component add rustfmt` and exited 127 on an image that ships
        # no rustup; that is FIXED - its header now reads "PROBE, DO NOT ADD"
        # and it probes `cargo fmt --version` first. What still blocks adoption
        # is its line 40: `cargo clippy --all-targets --all-features`, with
        # --all-features hard-coded and no knob. This image cannot build
        # --all-features (gui_unix needs GTK4 headers, the onnxruntime features
        # need vendor SDKs, and uid 1001 cannot apt-get install either), so the
        # driver can only fail here.
        #
        # SWITCH TO IT the moment the driver takes a CARGO_CLIPPY_ARGS knob:
        # delete this case body and add `fmt-clippy)` to the list above. Until
        # then these are the two exact commands AGENTS.md and README.md tell a
        # human to run, so a green local run means a green lane.
        #
        # Default features on purpose - see above. A lint gate that cannot run
        # is worse than a narrower one that does: this step was
        # `continue-on-error: true` for months, exited 127 every time, and let
        # a whole crate reach the default branch unformatted and with 12 clippy
        # errors.
        cargo fmt --all -- --check
        exec cargo clippy --workspace --all-targets --locked "$@" -- -D warnings
        ;;

    -h|--help|help) usage; exit 0 ;;
    *)
        echo "ci-container-steps.sh: unknown step '${step}'" >&2
        usage
        exit 2
        ;;
esac
