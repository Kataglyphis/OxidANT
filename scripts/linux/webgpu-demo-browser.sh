#!/usr/bin/env bash
# webgpu-demo-browser.sh [webgpu-demo-browser.js options]: builds the web demo, then draws every mode in headless Chrome. See docs/testing.md#the-web-demo-in-a-browser
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd -- "${script_dir}/../.." && pwd)"
cd "${root}"
web="crates/webgpu_renderer/web"

# cargo install puts the pinned CLI here, which the image's PATH leaves out.
export PATH="${CARGO_HOME:-${HOME}/.cargo}/bin:${PATH}"
# wasm-bindgen-cli refuses a module from any other wasm-bindgen than its own, so it is pinned to Cargo.lock's.
wb_ver="$(tr -d '\r' <Cargo.lock | awk '$0 == "name = \"wasm-bindgen\"" { getline; if (sub(/^version = "/, "") && sub(/"$/, "")) print; exit }')"
[ -n "${wb_ver}" ] || { echo "webgpu-demo-browser.sh: no wasm-bindgen in Cargo.lock" >&2; exit 2; }
if [ "$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')" != "${wb_ver}" ]; then
    cargo install --locked wasm-bindgen-cli --version "${wb_ver}"
fi

# The release profile, as the docs site publishes it.
cargo build --locked -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown --release
target_dir="$(cargo metadata --locked --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
wasm-bindgen "${target_dir}/wasm32-unknown-unknown/release/kataglyphis_webgpu_renderer.wasm" \
    --out-dir "${web}/pkg" --target web
exec node "${script_dir}/webgpu-demo-browser.js" "${web}" "$@"
