#!/usr/bin/env bash
# webgpu-demo-browser.sh [webgpu-demo-browser.js options]: builds the web demo, then draws every mode in headless Chrome. See docs/testing.md#the-web-demo-in-a-browser
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/antfrastructure.sh
source "${script_dir}/lib/antfrastructure.sh"
# shellcheck source=lib/cargo-target.sh
source "${script_dir}/lib/cargo-target.sh"
cd "${KATAGLYPHIS_REPO_ROOT}"
web="crates/webgpu_renderer/web"

# cargo install puts the pinned CLI here, which the image's PATH leaves out.
export PATH="${CARGO_HOME:-${HOME}/.cargo}/bin:${PATH}"
antfrastructure_source linux/scripts/lib/rust-toolchain.sh
ensure_wasm_bindgen Cargo.lock || exit 2

# The release profile, as the docs site publishes it.
target_dir="$(cargo_target_dir)"
wasm_web_build kataglyphis_webgpu_renderer "${web}/pkg" "${target_dir}"
exec node "${script_dir}/webgpu-demo-browser.js" "${web}" "$@"
