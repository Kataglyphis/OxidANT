#!/usr/bin/env bash
# renovate-local.sh - what this repo's dependencies are behind on, decided by
# Renovate run as a LOCAL CLI, plus the git half that moves the gitlink.
#
#   bash scripts/linux/renovate-local.sh                    # report (default)
#   bash scripts/linux/renovate-local.sh --managers cargo   # the crate pins
#   bash scripts/linux/renovate-local.sh --apply --dry-run  # show the plan
#   bash scripts/linux/renovate-local.sh --apply            # move the gitlink
#   bash scripts/linux/renovate-local.sh --print-bin        # resolved renovate.js
#
# THIN WRAPPER over ANTfrastructure linux/scripts/renovate-local.sh, exactly as
# run-lint-gates.sh next to it wraps the shared gate runner. Upstream owns all
# of it: the on-demand, checksum-verified bootstrap of RENOVATE_NODE_VERSION and
# RENOVATE_VERSION (both pinned in the hub's linux/scripts/01-core/versions.env,
# and RENOVATE_NODE_VERSION is deliberately NOT the canonical NODE_VERSION -
# Renovate declares engines.node "^24.11.0" while the images ship 26), the run
# against this repo's own .github/renovate.json, the machine-readable report it
# parses, and the apply half.
#
# THE CONSUMER ROOT IS PASSED EXPLICITLY, the same rule run-lint-gates.sh
# follows: upstream defaults its target to $PWD, so a run from crates/ or from
# inside third_party/ANTfrastructure would grade the wrong tree and answer with a
# cheerful "up to date". That also means you must not pass a root of your own -
# a second one is "more than one repo root given" upstream. Every other flag is
# forwarded untouched.
#
# ONLY ONE OF THE TWO HALVES WRITES. Renovate's --platform=local forces dryRun:
# it DETECTS and never edits a file, so a report run leaving Cargo.toml
# byte-identical is the tool working. The write half is git, and it moves
# GITLINKS only - explicit paths, only for submodules that declare a `branch =`.
# Here that is the single entry in .gitmodules, third_party/ANTfrastructure
# (branch = main): the pin both CI lanes run their gates from, and the one whose
# drift silently changes them. Nothing is staged or committed.
#
# First run, 2026-09-09 from WSL: one row for third_party/ANTfrastructure, in about
# four seconds. The shas are deliberately NOT written down here: the report is
# rendered from Renovate's cache, so the pair it prints is whatever that cache
# held, not necessarily the branch tip -- a number frozen in this header would
# read as a fact and be one only by accident. That is the whole default scope in this
# repo, and it is the reason the wrapper is worth its lines: the gitlink that
# decides what both lanes run had drifted again with nothing watching it.
#
# CARGO IS REPORT-ONLY, and this repo is where that gap is widest: eight tracked
# Cargo.toml files against one gitlink. `--managers cargo` tells you what is
# behind across the workspace; --apply will not touch it. Editing the manifests
# is still `cargo upgrade` (README § Dependencies) or Dependabot's PRs -
# .github/dependabot.yml is deliberately still present and covers cargo plus the
# pinned actions.
#
# READING THAT CARGO TABLE, measured here 2026-09-09 (`--managers cargo`, from
# WSL): 21 rows, and only ONE of them shows two different strings -
# flutter_rust_bridge `=2.12.0` -> `=2.13.0`. The other twenty print the same
# value twice (`wgpu 30 30`, `egui 0.36 0.36`). They are not noise and not a
# bug: the shared report prints the manifest's currentValue and newValue, and
# for a RANGE dep that already covers the new release the manifest string does
# not change - only the lock does. The report JSON carries the numbers the table
# drops; the same run in the sibling repo showed `cxx` at currentVersion
# 1.0.194, newVersion 1.0.200, updateType patch, with newValue still `1.0`. So a
# same-string row means "`cargo update` would move this", and an exact pin like
# flutter_rust_bridge's is the kind that needs a manifest edit.
#
# THE GITHUB-BACKED MANAGERS CAN ANSWER SHORT WITHOUT A TOKEN. `git-submodules`
# never needs one - it is anonymous `git ls-remote` - but the managers that reach
# api.github.com are rate-limited without one, and Renovate says so rather than
# failing: "Rate limit exceeded for api.github.com, as no hostRules set for this
# host" (seen in the sibling AccelerANTgine run on 2026-09-09). The variable that
# clears it under `--platform=local` is GITHUB_COM_TOKEN, and RENOVATE_TOKEN
# alone does NOT - measured both ways in ANThology the same day, where supplying
# it took a `github-actions` report from 1 row to 5:
#
#   GITHUB_COM_TOKEN="$(gh auth token)" bash scripts/linux/renovate-local.sh --managers cargo
#
# The `--managers cargo` run above needed no token to produce its 21 rows, so
# treat this as "add it when a run warns", not as a precondition.
#
# NOTHING RUNS THIS FOR YOU. The Renovate GitHub App is installed on no repo in
# this family and will not be (owner decision, 2026-09-09), so this CLI is the
# only thing that ever reads the tracked .github/renovate.json. No workflow calls
# this script and it blocks no commit; it is not a gate.
#
# ON THIS HOST, RUN IT FROM WSL: the bootstrap wants Node and there is none on
# the Windows side. That stands a Linux git next to a Windows checkout, which
# reads every text file as CR-modified - so --apply would abort part way through
# with the superproject already half moved. Upstream settles that itself: it
# switches to git.exe when WSL can reach the tree through it, and REFUSES up
# front when it cannot. The report half only reads and is safe from anywhere.
#
# Rationale, the version pins and the full-fidelity `--platform=github` variant:
# third_party/ANTfrastructure/docs/dependency-updates.md - read its token paragraph
# against the GITHUB_COM_TOKEN measurement above.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${SCRIPT_DIR}/lib/antfrastructure.sh"

# Named separately from antfrastructure_path's generic "not found / it moved
# upstream" message: while the family adopts this tool the expected failure is a
# gitlink pinned BEFORE the driver existed upstream, and being sent to
# docs/INDEX.md to look for a file that is simply not in this pin wastes the
# trip. The sibling wrappers in OmniAccelerANT and jotrockenmitlocken say the
# same thing for the same reason.
HUB_RENOVATE_RELATIVE="linux/scripts/renovate-local.sh"
if [ ! -f "${ANTFRASTRUCTURE_DIR}/${HUB_RENOVATE_RELATIVE}" ]; then
  echo "Error: ${ANTFRASTRUCTURE_DIR}/${HUB_RENOVATE_RELATIVE} is missing." >&2
  echo "       Either ANTfrastructure is not checked out (git submodule update" >&2
  echo "       --init --recursive third_party/ANTfrastructure), or the pinned" >&2
  echo "       ANTfrastructure predates the shared Renovate CLI - bump the" >&2
  echo "       third_party/ANTfrastructure gitlink." >&2
  exit 1
fi

antfrastructure_exec "${HUB_RENOVATE_RELATIVE}" "${KATAGLYPHIS_REPO_ROOT}" "$@"
