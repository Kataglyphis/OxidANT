#!/usr/bin/env bash
# Fake generator and Cargo around the real bridge script; the real codegen run is the fmt-clippy CI step.
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/antfrastructure.sh
source "${SCRIPT_DIR}/../lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/test-harness.sh || exit 1
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT
repo="${work}/repo"
hub="${work}/hub"
mkdir -p "${work}/bin" "${work}/tmp" "${repo}/scripts/linux" "${repo}/src" "${hub}/linux/scripts/01-core"
cp "${SCRIPT_DIR}/../frb-codegen.sh" "${SCRIPT_DIR}/../verify-frb-bindings.py" "${repo}/scripts/linux/"
printf 'FLUTTER_RUST_BRIDGE_VERSION=2.13.0\n' > "${hub}/linux/scripts/01-core/versions.env"
cat > "${hub}/linux/scripts/01-core/downloads.sh" <<'SH'
download_verified_file() {
    printf 'DOWNLOAD %s %s\n' "$1" "$2" >> "${FRB_LOG}"
    mkdir -p "${FRB_TMP_CRATE}/cargo-expand-1.0.127"
    tar -czf "$3" -C "${FRB_TMP_CRATE}" cargo-expand-1.0.127
}
SH
cat > "${repo}/Cargo.toml" <<'TOML'
[workspace.dependencies]
flutter_rust_bridge = "=2.13.0"
[lib]
name = "oxidant"
TOML
fresh_rust() {
    printf 'pub(crate) const FLUTTER_RUST_BRIDGE_CODEGEN_VERSION: &str = "2.13.0";\n'
    printf 'pub(crate) const FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = 42;\n'
}
fresh_rust > "${repo}/src/frb_generated.rs"
git -C "${repo}" init -q
t_git_commit "${repo}"

cat > "${work}/bin/cargo" <<'SH'
#!/usr/bin/env bash
printf 'CARGO %s\n' "$*" >> "${FRB_LOG}"
case "$1" in
    metadata) exit "${FAKE_METADATA_RC:-0}" ;;
    expand) printf 'cargo-expand %s\n' "${FAKE_EXPAND_VERSION:-1.0.127}" ;;
    install) exit 0 ;;
    *) exit 2 ;;
esac
SH
cat > "${work}/bin/flutter_rust_bridge_codegen" <<'SH'
#!/usr/bin/env bash
case "$1" in
    --version) echo "flutter_rust_bridge_codegen ${FAKE_CODEGEN_VERSION:-2.13.0}"; exit 0 ;;
    generate) echo "GENERATE $*" >> "${FRB_LOG}"; shift ;;
    *) exit 2 ;;
esac
while [ "$#" -gt 0 ]; do
    case "$1" in
        --rust-root) rust_root="$2"; shift 2 ;;
        --dart-output) dart_output="$2"; shift 2 ;;
        *) shift ;;
    esac
done
mkdir -p "${dart_output}"
[ -f "${dart_output}/../../../pubspec.yaml" ] && sed -n 's/^name: /PACKAGE /p' "${dart_output}/../../../pubspec.yaml" >> "${FRB_LOG}"
printf "String get codegenVersion => '2.13.0';\nint get rustContentHash => %s;\nstem: '%s',\n" \
    "${FAKE_HASH:-42}" "${FAKE_STEM:-oxidant}" > "${dart_output}/frb_generated.dart"
printf 'pub(crate) const FLUTTER_RUST_BRIDGE_CODEGEN_VERSION: &str = "2.13.0";\n' > "${rust_root}/src/frb_generated.rs"
printf 'pub(crate) const FLUTTER_RUST_BRIDGE_CODEGEN_CONTENT_HASH: i32 = %s;\n' "${FAKE_HASH:-42}" >> "${rust_root}/src/frb_generated.rs"
exit "${FAKE_GENERATE_RC:-0}"
SH
chmod +x "${work}/bin/cargo" "${work}/bin/flutter_rust_bridge_codegen"
export FRB_LOG="${work}/log" FRB_TMP_CRATE="${work}/crate" PATH="${work}/bin:${PATH}" TMPDIR="${work}/tmp"

run() {
    : > "${FRB_LOG}"
    (cd "${repo}" && "$@" bash scripts/linux/frb-codegen.sh check --hub "${hub}") > "${work}/out" 2>&1
    printf '%s\n' "$?" > "${work}/rc"
    git -C "${repo}" checkout -q -- src/frb_generated.rs
}

run env
t_case "fresh bindings pass after full locked metadata and the one flag set"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${FRB_LOG}" '^CARGO metadata --locked --manifest-path .*/Cargo.toml --format-version 1$'
t_assert_no_grep "${FRB_LOG}" 'filter-platform'
t_assert_grep "${FRB_LOG}" '^GENERATE generate --rust-root .* --rust-input crate::api --dart-output .*/lib/src/rust --no-deps-check --no-build-runner --no-dart-fix --no-add-mod-to-lib --no-auto-upgrade-dependency --stop-on-error$'
t_case "the temporary Dart package carries the consumer's name, which prefixes the Rust symbols"
t_assert_grep "${FRB_LOG}" '^PACKAGE omni_accelerant$'
t_case "the temporary Dart package is removed"
t_assert_eq "" "$(ls -A "${TMPDIR}")"
t_case "the pinned cargo-expand is reused, not reinstalled"
t_assert_no_grep "${FRB_LOG}" '^CARGO install'

run env FAKE_HASH=43
t_case "a Rust side that differs from the committed file fails"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" 'src/frb_generated.rs is stale'

run env FAKE_STEM=UNKNOWN
t_case "an UNKNOWN loader stem fails"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" "loader does not match Rust library: \['UNKNOWN'\]"

run env FAKE_STEM=other
t_case "a loader stem other than the Rust library name fails"
t_assert_grep "${work}/rc" '^1$'

run env FAKE_METADATA_RC=101
t_case "failed cross-target metadata stops before the generator"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" 'full cross-target cargo metadata failed'
t_assert_no_grep "${FRB_LOG}" '^GENERATE'

run env FAKE_CODEGEN_VERSION=2.12.0
t_case "a generator other than the hub's pin stops before metadata"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" "generator is 'flutter_rust_bridge_codegen 2.12.0', the hub pins 2.13.0"
t_assert_no_grep "${FRB_LOG}" '^(CARGO|GENERATE)'

run env FAKE_GENERATE_RC=3
t_case "a failing generator fails the check"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" 'flutter_rust_bridge_codegen generate failed'

run env FAKE_EXPAND_VERSION=1.0.126
t_case "another cargo-expand is replaced by the checksummed pin"
t_assert_grep "${work}/rc" '^0$'
t_assert_grep "${FRB_LOG}" '^DOWNLOAD https://static.crates.io/crates/cargo-expand/cargo-expand-1.0.127.crate c72c8589dbf5399e67fd6dc66710490ea7ca2c2b898961131dbb7e42c0f3dd9c$'
t_assert_grep "${FRB_LOG}" '^CARGO install --locked --path .*/cargo-expand-1.0.127$'

printf 'FLUTTER_RUST_BRIDGE_VERSION=2.14.0\n' > "${hub}/linux/scripts/01-core/versions.env"
run env
t_case "a runtime pin that differs from the hub's generator pin fails"
t_assert_grep "${work}/rc" '^1$'
t_assert_grep "${work}/out" 'runtime 2.13.0 differs from the hub pin 2.14.0'
printf 'FLUTTER_RUST_BRIDGE_VERSION=2.13.0\n' > "${hub}/linux/scripts/01-core/versions.env"

mkdir -p "${work}/app/lib/src/rust"
: > "${FRB_LOG}"
rc=0
(cd "${work}" && bash repo/scripts/linux/frb-codegen.sh generate --hub "${hub}" --rust-root repo --dart-output app/lib/src/rust) > "${work}/out" 2>&1 || rc=$?
t_case "generate writes both sides into the roots it is given"
t_assert_eq 0 "${rc}"
t_assert_grep "${work}/app/lib/src/rust/frb_generated.dart" "^stem: 'oxidant',$"
t_assert_grep "${FRB_LOG}" '^GENERATE generate --rust-root repo --rust-input crate::api --dart-output app/lib/src/rust '
t_summary
