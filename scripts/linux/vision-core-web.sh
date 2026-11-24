#!/usr/bin/env bash
# vision-core-web.sh [pkg dir]: builds the browser's vision module for wasm32 and checks ModelANT's contract on it in node, bit for bit against native. See docs/testing.md#vision_core-in-the-browser
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd -- "${script_dir}/../.." && pwd)"
cd "${root}"

# cargo install puts the pinned CLI here, which the image's PATH leaves out.
export PATH="${CARGO_HOME:-${HOME}/.cargo}/bin:${PATH}"
# shellcheck source=lib/antfrastructure.sh
source "${script_dir}/lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/rust-toolchain.sh
# A bare runner's rustup has the host target only, and rustc then fails on cfg-if with "can't find crate for core".
ensure_wasm32_target || exit 2
ensure_wasm_bindgen Cargo.lock || exit 2

contract="third_party/ModelANT/contracts/inference-v1.json"
target_dir="$(cargo metadata --locked --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
pkg="${1:-${target_dir}/vision-core-web/pkg}"

# The release profile, as a site ships it.
cargo build --locked -p kataglyphis_vision_core_web --target wasm32-unknown-unknown --release
wasm-bindgen "${target_dir}/wasm32-unknown-unknown/release/kataglyphis_vision_core_web.wasm" \
    --out-dir "${pkg}" --target web
cargo run --locked --quiet -p kataglyphis_vision_core_web --example contract_report -- "${contract}" \
    > "${pkg}/native-report.json"
exec node "${script_dir}/vision-core-web-contract.mjs" "${pkg}" "${contract}" "${pkg}/native-report.json"
