# Backlog

For the ANTfrastructure agentic loop (`shared/agentic-loop/`), once this repo adopts it
(see *Not adopted yet*). The file was once zero bytes, which read as "the protocol exists
and the backlog is empty"; it was neither.

## Protocol

- `- [ ]` actionable — the planner may pick it up
- `- [b]` blocked — skipped, and excluded from the pending count, so a
  backlog containing only blocked items still lets the planner run again
- `- [x]` completed — pruned on sight; the history lives in git

## Open

- [b] Drop the vendored `third_party/egui-winit-0.36.2` and its
      `[patch.crates-io]` entry once egui 0.37 (or any release carrying
      emilk/egui#8516) is on crates.io, then move the egui family to it.
      Blocked on that release. Steps: `third_party/egui-winit-0.36.2/PATCHED.md`.
      Re-checked 2026-10-12: crates.io's newest `egui` and `egui-winit` are still 0.36.2, published 2026-09-08 (emilk/egui#8516 merged 2026-09-07, unreleased).

- [b] Instanced normals shade differently from the equivalent node transform:
      `a_non_uniform_instance_scale_shades_like_the_same_node_scale` fails with
      987 differing pixels against a threshold of 40, pre-existing and
      deterministic, also on the image's lavapipe (2026-09-29). It is `#[ignore]`d
      so the Linux lanes can run the golden suite with `KATAGLYPHIS_REQUIRE_GPU=1`;
      drop the attribute with the fix. Blocked here: `src/shaders/*.wgsl` are
      generated, and the `forward.slang` source lives in the C++ engine repo.
      Detail: AGENTS.md § Known gaps.
- [b] Ship the .slang sources with crates/webgpu_renderer, or add a
      slang -> wgsl step, so "there is no .slang file in this repo" stops
      being true and the item above becomes fixable here (hand-editing the
      generated WGSL desynchronises it from its source). Blocked on deciding
      who compiles tex_quad.slang.
- [ ] The Windows lanes' real-model tests pass by skipping. `Build-Windows.ps1` runs
      *Unit Tests* before *Fetch Models*, so `tests/real_model.rs` finds no model and
      reports `ok` (`cargo test` hides a passing test's `SKIP:` line), and neither
      lane sets `KATAGLYPHIS_REQUIRE_MODEL`. The Linux and riscv64 steps fetch first and
      demand it. Moving the fetch ahead of the tests is the easy half; whether the test
      binary then finds the chain ORT before *Stage Chain ONNX Runtime* has to be seen on
      the Windows box first (found 2026-10-15).
- [ ] The headless renderer tests segfault now and then when their binary runs its tests on
      parallel threads on the dev host (Ryzen 9950X, AVX-512), at the current pin and at an older
      one; run serially they pass, and CI is green (found 2026-10-19). Unlike the arm64 WARP crash
      (AGENTS.md § Continuous integration), no driver is named yet: check the renderer's and the test
      fixture's thread safety first.
- [ ] The docs site carries no guide. `docs.yml` builds rustdoc alone (`builder: rustdoc`), so
      the eight `docs/*.md` pages reach readers only on GitHub. The hub's rustdoc builder takes no
      guide list (its `builder: dartdoc` does since hub c91187a9): give it one, or render the
      guides as crate docs.
- [b] Build the GTK4 GUI on Linux (owner 2026-10-20: both GUIs, no switch). Blocked on the
      family Linux image: `:latest` (2026-10-11 build) has only `/opt/gstreamer`'s `gtk4.pc`,
      which names a `graphene-gobject-1.0` it lacks, so `pkg-config --cflags gtk4` fails. Hub
      da25fa01 (CON129) adds GTK4 to the next image (amd64: GTK 4.14.5 in `/opt/gstreamer`;
      arm64, riscv64: Ubuntu's 4.22.4; the hub checked the GUI with it on amd64 and arm64 and built
      the CLI on amd64). Once
      that image is published, the change is:
      `crates/gui/Cargo.toml` gets
      `[target.'cfg(target_os = "linux")'.dependencies] gtk = { version = "0.11", package = "gtk4", features = ["v4_10"] }`,
      `crates/gui/src/lib.rs` gets `#[cfg(target_os = "linux")] pub mod gui;` (the module,
      `crates/gui/src/gui.rs`, is in the tree; no CI lane ever compiled it), and the CLI a `gui-gtk` subcommand under the same cfg
      calling `kataglyphis_gui::gui::run()`. The lockfile then regains gtk4 0.11's graph,
      pruned on 2026-10-20, and `scripts/linux/package-linux.sh` its GTK4 closure and the
      `gtk4paintablesink` plugin, or `autovideosink` stays the sink.

## Not adopted yet

The loop itself (config, runner wrappers, `scripts/agentic-loop/`) is not set up.
Copy-and-edit templates live in ANTfrastructure's `shared/agentic-loop/templates/`; a
consumer supplies this file, a config JSON, thin runner wrappers, and optionally
per-engine system prompts.
