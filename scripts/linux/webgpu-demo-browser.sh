#!/usr/bin/env bash
# webgpu-demo-browser.sh [webgpu-demo-browser.js options]: builds the web demo, then draws every mode in headless Chrome. See docs/testing.md#the-web-demo-in-a-browser
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd -- "${script_dir}/../.." && pwd)"
cd "${root}"
web="crates/webgpu_renderer/web"

# cargo install puts the pinned CLI here, which the image's PATH leaves out.
export PATH="${CARGO_HOME:-${HOME}/.cargo}/bin:${PATH}"
# shellcheck source=lib/antfrastructure.sh
source "${script_dir}/lib/antfrastructure.sh"
antfrastructure_source linux/scripts/lib/rust-toolchain.sh
ensure_wasm_bindgen Cargo.lock || exit 2

# The release profile, as the docs site publishes it.
cargo build --locked -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown --release
target_dir="$(cargo metadata --locked --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
wasm-bindgen "${target_dir}/wasm32-unknown-unknown/release/kataglyphis_webgpu_renderer.wasm" \
    --out-dir "${web}/pkg" --target web
exec node "${script_dir}/webgpu-demo-browser.js" "${web}" "$@"
