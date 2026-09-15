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

- [b] Instanced normals shade differently from the equivalent node transform.
      `a_non_uniform_instance_scale_shades_like_the_same_node_scale` fails with
      987 differing pixels against a threshold of 40; confirmed pre-existing and
      deterministic. Blocked here: `src/shaders/*.wgsl` are generated artifacts
      and the `forward.slang` source lives in the C++ engine repo.
- [b] The ARM lane cannot go green — `:latest-cross` resolves to an amd64-only
      manifest list. Blocked on ANTfrastructure
      (`build-runtime-manifest.sh --repair --push-manifest`).
- [ ] Give CI a GPU adapter (software is enough: `mesa-vulkan-drivers` plus
      `KATAGLYPHIS_REQUIRE_GPU=1`) so the ~40 headless golden tests stop
      silently skipping and reporting as passed.

- [b] Bring the renderer design docs here (decision D6). Copy
      renderer-bounds-invariant.md, webgpu-renderer-roadmap.md and
      webgpu-gltf-rust-plan.md out of BeschleunigerBallett/docs/ into
      crates/webgpu_renderer/docs/, delete them there, leave a pointer, and
      turn this repo's three absolute URLs into crate-relative docs/<name>.md.
      Blocked here: it is a cross-repository move and the other half is
      BeschleunigerBallett's. The table in AGENTS.md says which four stay
      external.

- [b] Ship the .slang sources with crates/webgpu_renderer, or add a
      slang -> wgsl step, so "there is no .slang file in this repo" stops
      being true. src/shaders/*.wgsl are checked-in GENERATED artifacts whose
      source is the C++ engine's forward.slang; hand-editing the WGSL
      desynchronises it, which is why the instanced-normal bug above cannot be
      fixed here. Blocked on deciding who compiles tex_quad.slang.

- [ ] Migrate crates/webgpu_renderer off `chunks_exact(N)` with a literal N.
      clippy 1.98 added `chunks_exact_to_as_chunks` and it fires 22 times there
      (glTF loader, OBJ converter, HDR decoder, gpu_timing, histogram, ibl,
      occlusion, lod, qem). It is allowed once in the root Cargo.toml's
      `[workspace.lints.clippy]` with a reason; the migration changes the
      iterated element type from `&[T]` to `&[T; N]` at every site, so it wants
      its own change and its own review. `cargo clippy --fix` does not do it:
      it rewrites the tests and leaves every src site.

- [ ] The Ubuntu lane can only produce a tarball. ANTfrastructure's
      `package_archive.sh` writes the tar and stops; its `create_deb()` was
      deleted on 2026-08-08 as unreachable, and `--flatpak-manifest`,
      `--desktop-file` and `--appdata-file` are checked for existence and then
      never read. `PACKAGE_TYPES` now says `tar`, which is honest but narrow.
      Restoring deb/AppImage/Flatpak means changing ANTfrastructure, not this repo
      -- the packaging/flatpak/ files here are ready and unused.

## Not adopted yet

The loop itself — config, runner wrappers, `scripts/agentic-loop/` — is not set
up. Copy-and-edit templates live in ANTfrastructure's
`shared/agentic-loop/templates/`; a consumer supplies this file, a config JSON,
thin runner wrappers, and optionally per-engine system prompts.
