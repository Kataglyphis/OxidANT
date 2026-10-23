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

- [ ] **Diagnose the intermittent headless GPU-test SIGSEGV** (M). A full default-parallel
      workspace run at `38b1c597` crashed in the `occlusion` test binary after 7/11 cases
      (SIGSEGV 11, exit 101). The same ELF then passed default and serial runs and five
      further default repeats; source CI on `920ca1b7` passed. The crash remains unexplained.
      Next: capture the GPU backend/driver and all-thread stack or core, distinguish test
      resource lifetime from a driver fault, then add a regression for the proven cause.
      Do not serialize all tests or call it fixed solely because CI is green.

## Not adopted yet

The loop itself (config, runner wrappers, `scripts/agentic-loop/`) is not set up.
Copy-and-edit templates live in ANTfrastructure's `shared/agentic-loop/templates/`; a
consumer supplies this file, a config JSON, thin runner wrappers, and optionally
per-engine system prompts.
