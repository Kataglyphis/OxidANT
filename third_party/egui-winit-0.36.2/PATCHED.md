# egui-winit 0.36.2, patched for wasm32 (temporary)

This directory is the published `egui-winit` 0.36.2 crate with one upstream fix
applied. The root `Cargo.toml` routes `egui-winit` here through
`[patch.crates-io]`.

## Why

No published `egui-winit` compiles for `wasm32-unknown-unknown`. On wasm32,
`egui::DroppedFile` requires `bytes_async`, but `egui-winit`'s `NativeFile`
implements only `bytes` ([egui#8436](https://github.com/emilk/egui/issues/8436)).
The renderer's wasm demo (`crates/webgpu_renderer`, `src/wasm_demo.rs`) drives
its egui overlay through `egui_winit::State` on winit's web backend. So
BeschleunigerBallett's "docs + wasm demo" job, which builds the renderer for
wasm32 as a fatal size-budget gate, failed with `error[E0407]: method bytes is
not a member of trait egui::DroppedFile` (run 36020443791, 2026-09-24).

## What was applied

- **Source:** `https://static.crates.io/crates/egui-winit/egui-winit-0.36.2.crate`,
  sha256 `98466000559d66ba4db786a30448b01483946add4188e7f10f522a27b4fb89cc`. The
  checksum matches the crates.io index entry for 0.36.2.
- **Change:** the `src/lib.rs` part of
  [egui#8516](https://github.com/emilk/egui/pull/8516), "Fix egui-winit compilation
  on wasm32" (merged to egui's main on 2026-09-07, not in 0.36.2), applied unchanged
  with `patch -p1`. Hunks 3 and 4 applied at an offset of -6 lines.
  - `mod dropped_file` and `use dropped_file::NativeFile` are compiled only when
    not on wasm32.
  - On wasm32, `WindowEvent::DroppedFile` is ignored. Winit's web backend never
    emits it.
- Every other file is as published.

## Remove when

egui 0.37 (or any later release carrying #8516) is on crates.io. At that point:

1. Delete this directory and the `[patch.crates-io]` entry in the root `Cargo.toml`.
2. Move the egui family to that release.
3. Check that `cargo check -p kataglyphis_webgpu_renderer --target wasm32-unknown-unknown`
   still builds.
