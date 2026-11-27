#!/usr/bin/env bash
# Where cargo writes and the wasm32 browser build read from there, so no script reparses cargo metadata itself.

# cargo_target_dir: the workspace's target directory, wherever CARGO_TARGET_DIR or a cargo config moves it.
cargo_target_dir() {
    cargo metadata --locked --no-deps --format-version 1 \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

# wasm_web_build PACKAGE OUT_DIR TARGET_DIR: the package's release wasm32 module, bound for a browser into OUT_DIR.
wasm_web_build() {
    cargo build --locked -p "$1" --target wasm32-unknown-unknown --release || return $?
    wasm-bindgen "$3/wasm32-unknown-unknown/release/$1.wasm" --out-dir "$2" --target web
}
