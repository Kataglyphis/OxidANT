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
- [ ] Give CI a GPU adapter (software is enough: `mesa-vulkan-drivers` plus
      `KATAGLYPHIS_REQUIRE_GPU=1`) so the ~40 headless golden tests stop
      silently skipping and reporting as passed.

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

## Waiting on ANTfrastructure

Each of these is half-done here on purpose: the other half is a change to the
submodule, which is a different repository with other consumers. The local side
is written so that finishing it upstream is a deletion here, not a rewrite.

- [b] `_cargo_wrapper.sh` needs the safe.directory guard that
      `lib/cmake-build.sh:140-144` already has, behind a `CARGO_SAFE_DIRECTORY`
      knob defaulting to `/workspace`, and `cargo_release/bench/build_doc/`
      `coverage/security_checks.sh` should source it the way `cargo_debug.sh`
      does. Then drop the guard from `ci-container-steps.sh`. Re-checked against
      hub 19286e9f: still absent, still blocked.
- [b] `Get-ANTfrastructurePin` (hub `windows/scripts/rust/Build-Windows.ps1`)
      belongs in `WindowsScripts.Shared.psm1`, so this repo's
      `Resolve-CargoToolPin` in `scripts/windows/Build-Windows.ps1` can be
      deleted and both sides share one implementation. Re-checked against hub
      19286e9f: the function is still only in that one script, still blocked.
- [b] The MSI Packaging step of `scripts/windows/Build-Windows.ps1` should
      become a hub `windows/scripts/rust/New-MsiPackage.ps1` (or a
      `WindowsMsix.Common` function) taking `-WxsFile -LicenseFile
      -ProductName -Manufacturer -ExeSource -Version -OutFile`. The MSIX half
      of this landed upstream on 2026-09-15 as `Invoke-MsixPackage`, and this
      repo's ~100-line copy went with it; the MSI half has no hub function yet.
- [b] Decide the fate of the hub's `windows/scripts/rust/Build-Windows.ps1`:
      it has zero consumers, does `rustup component add` against an offline
      rustup and builds `--all-features`. Either make it callable
      (`-Features`/`-AllFeatures`, `-Package`/`-Bin`, opt-in benchmarks, no
      rustup calls, no scoop block) or delete it and record OxidANT as the
      owner of the Windows Rust build.
- [b] `docs/adopting-in-a-new-project.md` section 8 should list
      `scripts/windows/container/` as "scripts that run inside the Windows
      image" - the casing convention this repo now follows everywhere.
      Re-checked against hub 19286e9f: § 8 still does not name it.
- [b] The MSIX certificate trust dance (importing into `LocalMachine\Root`
      *and* `LocalMachine\TrustedPeople`, `0x800B0109`, `Get-AppxLog`) is
      still written out in this repo's README. It belongs in the hub's
      `windows/scripts/certificates/README.md`, which today covers only
      `TrustedPeople`. Re-checked against hub 19286e9f: unchanged.
- [b] The module inventory in AGENTS.md section 2 carries rows with no upstream
      owner (`WindowsMsix.Common`, `WindowsConfig.Common`, `WindowsBuild.Common`,
      `WindowsScripts.Shared`, the rust drivers, `package_archive.sh`, the
      composite actions, `lint-workflows.sh`, the agentic-loop templates, the
      01-core helpers). Once they are described in the hub's
      `docs/adopting-in-a-new-project.md` sections 2/8 or `docs/INDEX.md`, that
      table becomes a link. Re-checked against hub 19286e9f: only
      `WindowsMsix.Common` is named there (§ 7), so the table stays.

## Not adopted yet

The loop itself — config, runner wrappers, `scripts/agentic-loop/` — is not set
up. Copy-and-edit templates live in ANTfrastructure's
`shared/agentic-loop/templates/`; a consumer supplies this file, a config JSON,
thin runner wrappers, and optionally per-engine system prompts.
