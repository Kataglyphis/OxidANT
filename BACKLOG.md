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

## Not adopted yet

The loop itself (config, runner wrappers, `scripts/agentic-loop/`) is not set up.
Copy-and-edit templates live in ANTfrastructure's `shared/agentic-loop/templates/`; a
consumer supplies this file, a config JSON, thin runner wrappers, and optionally
per-engine system prompts.
