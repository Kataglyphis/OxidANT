#!/usr/bin/env bash
# run-lint-gates.sh - this repository's shell + workflow + secret lint gates.
#
# THIN WRAPPER over ContainerHub linux/scripts/run-lint-gates.sh, which owns the
# gates themselves, their pinned and SHA-verified bootstraps, the git-ls-files
# scope construction, the empty-list vacuity guards, the run-them-all-then-fail-
# once accumulator and the secret gate's self-test (an empty tree must scan
# clean, a planted PAT must be reported at the path that was passed in -
# otherwise "no findings" cannot be told apart from "the scanner never started").
#
# THIS CLOSES A GAP, IT DOES NOT REPLACE ANYTHING. Before this file OxidANT ran
# no gate over its SHELL, WORKFLOWS or SECRETS. It was not ungated entirely:
# rust_ubuntu26_04.yml:153-162 already enforces `cargo fmt --all -- --check` and
# `cargo clippy`, which is the Rust half and stays where it is.
# 4 tracked *.sh and 2 workflows went ungraded, and -
# the part that matters most - nothing ever scanned the tree for committed
# credentials, while rust_ubuntu26_04.yml publishes the docs over FTP with
# secrets.SERVER / secrets.USERNAME / secrets.PW.
#
# CI and a human run the SAME entry point, so the gate that blocks a merge can
# be reproduced on a dev box without pushing:
#
#   bash scripts/linux/run-lint-gates.sh   # the whole repo
#
# The consumer root is passed EXPLICITLY and is never inferred upstream: the
# hub half of this gate lives inside third_party/ContainerHub, so a root derived
# from its own location would grade ContainerHub's tree and report green over
# the wrong repository.
#
# Extra arguments are forwarded. The only one upstream takes is
# --exclude <top-level-dir>, and it REPLACES the third_party default rather than
# adding to it, so a narrower sweep has to name third_party again:
#
#   bash scripts/linux/run-lint-gates.sh --exclude third_party --exclude logs
#
# That default is what this repo wants unqualified: third_party/ContainerHub is
# a submodule, graded in its own repository at its own ratchet. Everything else
# here - including resources/ and logs/ - is OxidANT's own and stays in scope.
#
# The gate binaries are fetched on first use (pinned and SHA-verified upstream),
# so the first local run is not instant.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/containerhub.sh
source "${SCRIPT_DIR}/lib/containerhub.sh"

containerhub_exec linux/scripts/run-lint-gates.sh "${KATAGLYPHIS_REPO_ROOT}" "$@"
