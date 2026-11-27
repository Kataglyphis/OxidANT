#!/usr/bin/env bash
# cargo-target.sh against a fake cargo and wasm-bindgen; the real build and bindings are web-browser.yml's and the vision-web step's.
set -u
# shellcheck source=scripts/linux/tests/lib-suite.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib-suite.sh"
# shellcheck source=scripts/linux/lib/cargo-target.sh
source "${tests_dir}/../lib/cargo-target.sh"

mkdir -p "${work}/bin"
cat > "${work}/bin/cargo" <<'SH'
#!/usr/bin/env bash
printf 'CARGO %s\n' "$*" >> "${FAKE_LOG}"
case "$1" in
    metadata) printf '{"workspace_root": "/elsewhere", "target_directory": "%s"}\n' "${FAKE_TARGET}" ;;
    build) exit "${FAKE_BUILD_RC:-0}" ;;
esac
SH
cat > "${work}/bin/wasm-bindgen" <<'SH'
#!/usr/bin/env bash
printf 'BINDGEN %s\n' "$*" >> "${FAKE_LOG}"
SH
chmod +x "${work}/bin/cargo" "${work}/bin/wasm-bindgen"
export PATH="${work}/bin:${PATH}" FAKE_LOG="${work}/log" FAKE_TARGET="${work}/target dir"

t_case "cargo_target_dir prints the metadata's target directory, its space kept"
t_assert_eq "${FAKE_TARGET}" "$(cargo_target_dir)"

: > "${FAKE_LOG}"
suite_run wasm_web_build kataglyphis_demo "${work}/pkg" "${FAKE_TARGET}"
t_case "wasm_web_build binds the package's release wasm32 module for the web"
t_assert_eq 0 "$(cat "${work}/rc")"
t_assert_grep "${FAKE_LOG}" '^CARGO build --locked -p kataglyphis_demo --target wasm32-unknown-unknown --release$'
t_assert_grep "${FAKE_LOG}" "^BINDGEN ${FAKE_TARGET}/wasm32-unknown-unknown/release/kataglyphis_demo[.]wasm --out-dir ${work}/pkg --target web\$"

: > "${FAKE_LOG}"
export FAKE_BUILD_RC=3
suite_run wasm_web_build kataglyphis_demo "${work}/pkg" "${FAKE_TARGET}"
unset FAKE_BUILD_RC
t_case "a failed build returns its status and binds nothing"
t_assert_eq 3 "$(cat "${work}/rc")"
t_assert_no_grep "${FAKE_LOG}" '^BINDGEN'

t_summary
