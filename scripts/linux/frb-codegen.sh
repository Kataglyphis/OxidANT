#!/usr/bin/env bash
# The one flutter_rust_bridge input and flag set; it does not prove the Dart side compiles or the loader opens.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
FRB_RUST_INPUT='crate::api'
# The Dart package name prefixes every exported Rust symbol, so the check must use the consumer's.
FRB_DART_PACKAGE='omni_accelerant'
FRB_FLAGS=(--no-deps-check --no-build-runner --no-dart-fix --no-add-mod-to-lib --no-auto-upgrade-dependency --stop-on-error)
# The generator otherwise installs whatever cargo-expand is newest on first use.
CARGO_EXPAND_VERSION='1.0.127'
CARGO_EXPAND_SHA256='c72c8589dbf5399e67fd6dc66710490ea7ca2c2b898961131dbb7e42c0f3dd9c'

usage() {
    cat >&2 <<'USAGE'
usage: frb-codegen.sh generate --hub DIR --rust-root DIR --dart-output DIR
       frb-codegen.sh check --hub DIR

  generate  write both bridge sides: <rust-root>/src/frb_generated.rs and <dart-output>
  check     regenerate this checkout's src/frb_generated.rs (Dart into a temporary package)
            and fail when it differs from the committed file

--hub is the ANTfrastructure checkout whose versions.env pins the generator.
USAGE
}

die() { printf 'frb-codegen.sh: %s\n' "$*" >&2; exit 1; }

FRB_TEMP_DIRS=()
trap 'rm -rf "${FRB_TEMP_DIRS[@]}"' EXIT

ensure_cargo_expand() {
    local hub="$1" work
    [ "$(cargo expand --version 2>/dev/null || true)" != "cargo-expand ${CARGO_EXPAND_VERSION}" ] || return 0
    # shellcheck source=/dev/null
    source "${hub}/linux/scripts/01-core/downloads.sh"
    work="$(mktemp -d)"
    FRB_TEMP_DIRS+=("${work}")
    download_verified_file "https://static.crates.io/crates/cargo-expand/cargo-expand-${CARGO_EXPAND_VERSION}.crate" \
        "${CARGO_EXPAND_SHA256}" "${work}/cargo-expand.crate" || die "cargo-expand ${CARGO_EXPAND_VERSION} download failed"
    tar -xzf "${work}/cargo-expand.crate" -C "${work}" || die "cargo-expand ${CARGO_EXPAND_VERSION} crate did not unpack"
    cargo install --locked --path "${work}/cargo-expand-${CARGO_EXPAND_VERSION}" \
        || die "could not install cargo-expand ${CARGO_EXPAND_VERSION}"
}

pinned_version() {
    local value
    value="$(sed -n 's/^FLUTTER_RUST_BRIDGE_VERSION=\([0-9][0-9.]*\)$/\1/p' "$1/linux/scripts/01-core/versions.env")"
    [ -n "${value}" ] || die "no FLUTTER_RUST_BRIDGE_VERSION in $1/linux/scripts/01-core/versions.env"
    printf '%s\n' "${value}"
}

generate() {
    local hub="$1" rust_root="$2" dart_output="$3" pinned runtime actual
    pinned="$(pinned_version "${hub}")"
    runtime="$(python3 -I "${SCRIPT_DIR}/verify-frb-bindings.py" "${rust_root}/Cargo.toml" --print-version)"
    [ "${runtime}" = "${pinned}" ] || die "flutter_rust_bridge runtime ${runtime} differs from the hub pin ${pinned}"
    actual="$(flutter_rust_bridge_codegen --version)"
    [ "${actual}" = "flutter_rust_bridge_codegen ${pinned}" ] || die "generator is '${actual}', the hub pins ${pinned}"
    # No --filter-platform: the generator reads every target's dependencies and writes stem UNKNOWN when that fails.
    cargo metadata --locked --manifest-path "${rust_root}/Cargo.toml" --format-version 1 >/dev/null \
        || die "full cross-target cargo metadata failed"
    ensure_cargo_expand "${hub}"
    flutter_rust_bridge_codegen generate --rust-root "${rust_root}" --rust-input "${FRB_RUST_INPUT}" \
        --dart-output "${dart_output}" "${FRB_FLAGS[@]}" || die "flutter_rust_bridge_codegen generate failed"
    python3 -I "${SCRIPT_DIR}/verify-frb-bindings.py" "${rust_root}/Cargo.toml" \
        --dart "${dart_output}/frb_generated.dart" --rust "${rust_root}/src/frb_generated.rs" \
        || die "generated bindings disagree with the Rust library"
}

check() {
    local hub="$1" pinned
    pinned="$(pinned_version "${hub}")"
    FRB_CHECK_PACKAGE="$(mktemp -d)"
    FRB_TEMP_DIRS+=("${FRB_CHECK_PACKAGE}")
    printf 'name: %s\nenvironment:\n  sdk: ">=3.0.0 <4.0.0"\ndependencies:\n  flutter_rust_bridge: %s\n' \
        "${FRB_DART_PACKAGE}" "${pinned}" > "${FRB_CHECK_PACKAGE}/pubspec.yaml"
    generate "${hub}" "${REPO_ROOT}" "${FRB_CHECK_PACKAGE}/lib/src/rust"
    git -C "${REPO_ROOT}" diff --exit-code -- src/frb_generated.rs \
        || die "src/frb_generated.rs is stale: regenerate it (docs/testing.md § The bridge bindings)"
}

mode="${1-}"
[ "$#" -ge 1 ] || { usage; exit 2; }
shift
case "${mode}" in -h|--help|help) usage; exit 0 ;; esac
hub='' rust_root='' dart_output=''
while [ "$#" -gt 0 ]; do
    case "$1" in
        --hub) hub="${2:?--hub needs a directory}"; shift 2 ;;
        --rust-root) rust_root="${2:?--rust-root needs a directory}"; shift 2 ;;
        --dart-output) dart_output="${2:?--dart-output needs a directory}"; shift 2 ;;
        *) usage; exit 2 ;;
    esac
done
[ -n "${hub}" ] || { usage; exit 2; }
case "${mode}" in
    generate)
        [ -n "${rust_root}" ] && [ -n "${dart_output}" ] || { usage; exit 2; }
        generate "${hub}" "${rust_root}" "${dart_output}"
        ;;
    check) check "${hub}" ;;
    *) usage; exit 2 ;;
esac
