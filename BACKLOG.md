# Backlog

Consumed by the ANTfrastructure agentic loop (`shared/agentic-loop/`) when this
repo adopts it. The file was previously zero bytes, which reads as "the
protocol exists and the backlog is empty" — it was neither.

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
      Re-checked 2026-09-25: crates.io's newest `egui-winit` is still 0.36.2.

- [b] Instanced normals shade differently from the equivalent node transform.
      `a_non_uniform_instance_scale_shades_like_the_same_node_scale` fails with
      987 differing pixels against a threshold of 40; confirmed pre-existing and
      deterministic. Blocked here: `src/shaders/*.wgsl` are generated artifacts
      and the `forward.slang` source lives in the C++ engine repo.
      Reproduced byte-for-byte on the image's lavapipe (2026-09-29), where the
      Linux lanes now run the golden suite with `KATAGLYPHIS_REQUIRE_GPU=1`;
      the test is `#[ignore]`d for that, so drop the attribute with the fix.
- [b] Ship the .slang sources with crates/webgpu_renderer, or add a
      slang -> wgsl step, so "there is no .slang file in this repo" stops
      being true. src/shaders/*.wgsl are checked-in GENERATED artifacts whose
      source is the C++ engine's forward.slang; hand-editing the WGSL
      desynchronises it, which is why the instanced-normal bug above cannot be
      fixed here. Blocked on deciding who compiles tex_quad.slang.

- [ ] Find why the renderer's `headless.exe` dies on `windows-11-arm` when its
      tests run in parallel. Run 36889467167 (2026-10-01): WARP (`Microsoft
      Basic Render Driver`, Dx12), `KATAGLYPHIS_REQUIRE_GPU=1`, the process
      exited after `running 39 tests` with no test finished and no panic text,
      the first time any arm64 test rendered. The same binaries on the x64
      runner's WARP pass in parallel, and serially (`--test-threads=1`, set in
      `Stage-CrossTests.ps1`) all 38 pass on arm64 (run 36907253720). One
      sample each, so it may be a race in WARP's arm64 JIT or a flake. Done
      when a parallel arm64 run is green repeatedly and the flag goes, or the
      crash has a name (exit code, faulting module). Since hub `05f8d3e3` the
      hub's `Invoke-StagedTests.ps1` gives both. A binary that ends without a
      summary fails with its exit code as an NTSTATUS name (e.g. `0xC0000005
      STATUS_ACCESS_VIOLATION`), plus WER's faulting module, offset and
      exception code when the runner logged event 1000. So one parallel arm64
      run, without `--test-threads=1`, is enough to name it.
      That run is a dispatch, not a red develop: `gh workflow run
      windows-arm64-cross.yml -f parallel-renderer-tests=true` drops the flag
      from `tests.json` on the runner only (2026-10-06).

- [ ] Decide whether the Linux x64 feature check (`feature-matrix` in
      `linux-x64.yml`) should also run on every push. It is the one job still
      behind a marker (`[build-features]`), six extra image pulls per run (one
      per matrix row since the `gui_windows` row joined on 2026-09-24); the
      2026-09-24 always-on request named the x64, arm64 and Windows lanes
      only. As of 2026-09-25 it has never run: no commit message carries the
      marker and the repository has no `workflow_dispatch` run.
## Not adopted yet

The loop itself — config, runner wrappers, `scripts/agentic-loop/` — is not set
up. Copy-and-edit templates live in ANTfrastructure's
`shared/agentic-loop/templates/`; a consumer supplies this file, a config JSON,
thin runner wrappers, and optionally per-engine system prompts.
