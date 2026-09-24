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

- [ ] Get `windows-x64.yml` green. It runs on every push and PR since
      2026-09-24, but it had not executed in CI since 2026-08-07, when all
      four runs (commits 23f13aec and f0801be0) failed in *Run debug unit,
      integration, and fuzz tests* after 59-77 minutes. The last green run is 2026-07-22
      (122 minutes). Until a first always-on run says otherwise, expect this
      lane red; the fix belongs in `scripts/windows/`, not in an `if:` that
      turns the lane back into a `skipped` badge.
- [ ] Decide whether the Linux x64 feature check (`feature-matrix` in
      `linux-x64.yml`) should also run on every push. It is the one job still
      behind a marker (`[build-features]`), five extra image pulls per run;
      the 2026-09-24 always-on request named the x64, arm64 and Windows lanes
      only.
- [ ] The Linux lanes can only produce a tarball. ANTfrastructure's
      `package_archive.sh` writes the tar and stops; its `create_deb()` was
      deleted on 2026-08-08 as unreachable, and `--flatpak-manifest`,
      `--desktop-file` and `--appdata-file` are checked for existence and then
      never read. `PACKAGE_TYPES` now says `tar`, which is honest but narrow.
      Restoring deb/AppImage/Flatpak means changing ANTfrastructure, not this repo
      -- the packaging/flatpak/ files here are ready and unused.
      Re-checked against hub 49be50f0, which DID add a flatpak pair
      (`app_packaging_ensure_flatpak_runtime`,
      `app_packaging_package_cmake_install_flatpak`): it does not close this.
      Both live in `lib/app-packaging.sh` and neither is reachable from
      `06-packaging/package_archive.sh`, which is byte-identical across the
      bump -- still `PACKAGE_TYPES=tar`, still accepting `--flatpak-manifest`
      and never reading it. The new entry point also stages from
      `cmake --install <build_dir>` and asserts an executable at
      `<prefix>/bin/<project_name>`; this repo has no CMakeLists.txt and builds
      with cargo, so it would need a cargo-install-tree twin, not a caller.
      Re-checked again at hub `604294e2`, the one bump that touches
      `lib/app-packaging.sh` (it makes `ostree` a required flatpak tool and says
      so when it is missing): `06-packaging/package_archive.sh` is still
      byte-identical and still never reads `--flatpak-manifest`, so the row is
      unmoved.

## Waiting on ANTfrastructure

Each of these is half-done here on purpose: the other half is a change to the
submodule, which is a different repository with other consumers. The local side
is written so that finishing it upstream is a deletion here, not a rewrite.

Every row below still says "re-checked against hub 49be50f0" because that
re-check still holds at `604294e2`. The bump between the two pins touches nine
files -- `.github/workflows/python-ci-windows.yml`, `CHANGELOG.md`,
`docs/code-quality-tooling.md`, `docs/python-ci.md`, `docs/scripts/mutations.json`,
`docs/shared-script-libraries.md`, `linux/scripts/lib/app-packaging.sh` and two
`linux/scripts/tests/` suites -- and none of them is `_cargo_wrapper.sh`,
`WindowsScripts.Shared.psm1`, anything under `windows/scripts/rust/`,
`docs/adopting-in-a-new-project.md`, `docs/INDEX.md` or the certificates README.
What it DID close was not a row here but a comment: the `powershell-lint` job in
`lint-gates.yml` is a `uses:` now, which is the one thing `49be50f0` offered and
this repo declined.

- [b] Hub text still names this repo's retired workflow files. On
      2026-09-24 `rust_ubuntu26_04.yml` became `linux-x64.yml` +
      `linux-arm64.yml` over `reusable-linux.yml`, and `rust_windows2025.yml`
      became `windows-x64.yml`. Upstream mentions of the old names:
      `linux/scripts/workflow-conventions.allow` (the three OxidANT CENSUS
      rows - every count is 0 now, so they can be deleted; the consumer run
      prints `RATCHET ... can be lowered` until then), `docs/ftp-deploys.md`,
      `docs/shared-script-libraries.md`, `linux/scripts/shellcheck-warnings.allow`
      (the package_archive.sh row), `.github/consumers.json` (the
      windows/scripts/rust row's `why`),
      `linux/scripts/02-toolchain/rust/cargo_fmt_clippy.sh`,
      `windows/scripts/rust/New-Archive.ps1` and `CHANGELOG.md`.
      `docs/ci-build-triggers.md` still teaches `[build-win]`/`[build-arm]`,
      which this repo no longer reads. A hub change, not one to make here.
- [b] `_cargo_wrapper.sh` needs the safe.directory guard that
      `lib/cmake-build.sh:140-144` already has, behind a `CARGO_SAFE_DIRECTORY`
      knob defaulting to `/workspace`, and `cargo_release/bench/build_doc/`
      `coverage/security_checks.sh` should source it the way `cargo_debug.sh`
      does. Then drop the guard from `ci-container-steps.sh`. Re-checked against
      hub 49be50f0: `_cargo_wrapper.sh` still has no safe.directory line at all
      and `CARGO_SAFE_DIRECTORY` appears nowhere in the hub -- still blocked.
      That bump's `01-core/fix_bind_mount_ownership` is NOT this: it chowns a
      tree a container wrote back to the mount's uid:gid, which is a filesystem
      ownership problem. This one is git refusing a checkout for dubious
      ownership, which `git config --global --add safe.directory` fixes and
      `chown` does not. Nothing here is replaced by it.
- [b] `Get-ANTfrastructurePin` (hub `windows/scripts/rust/Build-Windows.ps1`)
      belongs in `WindowsScripts.Shared.psm1`, so this repo's
      `Resolve-CargoToolPin` in `scripts/windows/Build-Windows.ps1` can be
      deleted and both sides share one implementation. Re-checked against hub
      49be50f0: the function is still only in that one script, still blocked.
- [b] The MSI Packaging step of `scripts/windows/Build-Windows.ps1` should
      become a hub `windows/scripts/rust/New-MsiPackage.ps1` (or a
      `WindowsMsix.Common` function) taking `-WxsFile -LicenseFile
      -ProductName -Manufacturer -ExeSource -Version -OutFile`. The MSIX half
      of this landed upstream on 2026-09-15 as `Invoke-MsixPackage`, and this
      repo's ~100-line copy went with it; the MSI half has no hub function yet.
      Re-checked against hub 49be50f0: `windows/scripts/rust/` is still
      Build-Windows.ps1, New-Archive.ps1 and New-MsixPackage.ps1, with no
      `New-MsiPackage` anywhere in the tree -- still blocked.
- [b] Decide the fate of the hub's `windows/scripts/rust/Build-Windows.ps1`:
      it has zero consumers, does `rustup component add` against an offline
      rustup and builds `--all-features`. Either make it callable
      (`-Features`/`-AllFeatures`, `-Package`/`-Bin`, opt-in benchmarks, no
      rustup calls, no scoop block) or delete it and record OxidANT as the
      owner of the Windows Rust build. Re-checked against hub 49be50f0:
      unchanged, and still the sole home of `Get-ANTfrastructurePin` above, so
      the two rows are decided together.
- [b] `docs/adopting-in-a-new-project.md` section 8 should list
      `scripts/windows/container/` as "scripts that run inside the Windows
      image" - the casing convention this repo now follows everywhere.
      Re-checked against hub 49be50f0: the file is untouched by that bump and
      § 8 still does not name it.
- [b] The MSIX certificate trust dance (importing into `LocalMachine\Root`
      *and* `LocalMachine\TrustedPeople`, `0x800B0109`, `Get-AppxLog`) is
      still written out in this repo's README. It belongs in the hub's
      `windows/scripts/certificates/README.md`, which today covers only
      `TrustedPeople`. Re-checked against hub 49be50f0: unchanged.
- [b] The module inventory in AGENTS.md section 2 carries rows with no upstream
      owner (`WindowsMsix.Common`, `WindowsConfig.Common`, `WindowsBuild.Common`,
      `WindowsScripts.Shared`, the rust drivers, `package_archive.sh`, the
      composite actions, `lint-workflows.sh`, the agentic-loop templates, the
      01-core helpers). Once they are described in the hub's
      `docs/adopting-in-a-new-project.md` sections 2/8 or `docs/INDEX.md`, that
      table becomes a link. Re-checked against hub 49be50f0: neither
      `docs/adopting-in-a-new-project.md` nor `docs/INDEX.md` changed in that
      bump, so only `WindowsMsix.Common` is still named (§ 7) and the table
      stays. The bump's new `WindowsMediaRuntime.Common` adds no row: it stages
      a GStreamer/ONNX DLL closure next to a built exe. Since 2026-09-23
      `scripts/windows/` here stages exactly one runtime dependency itself:
      the chain-built ONNX Runtime (Build-Windows.ps1's *Stage Chain ONNX
      Runtime*, via the project-local `WindowsOrtPayload.Common`), because it
      ships a payload per package rather than a whole media closure. The proof
      is the hub's ORT census (G6, `Test-OrtProvenanceTree`), which needs the
      hub pin at its ORT single-source commit of 2026-09-23 or later.

## Not adopted yet

The loop itself — config, runner wrappers, `scripts/agentic-loop/` — is not set
up. Copy-and-edit templates live in ANTfrastructure's
`shared/agentic-loop/templates/`; a consumer supplies this file, a config JSON,
thin runner wrappers, and optionally per-engine system prompts.
