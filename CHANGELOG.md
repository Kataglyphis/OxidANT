# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

It also carries the **dated engineering history** that used to sit in `AGENTS.md`:
measured baselines and the post-mortems of three migrations. Those are records of
what happened on a date, not guidance about what to do now, and keeping them in
`AGENTS.md` is what made that file 490 lines over 19 headings. They were moved here
on 2026-09-15, verbatim. Nothing was deleted.

## [Unreleased]

### Added
- **The Windows exe loads the chain ONNX Runtime, and every package carries its
  DLL closure on both arches (owner decisions 2026-09-25).**
  - The CLI's `onnxruntime_directml`/`onnxruntime_cuda` (and `onnx_tract`) features
    now reach the GUI's inference (`kataglyphis_gui?/...`) and the root crate's
    `onnxruntime`. Until now nothing in the exe called ORT, so Build-Windows staged
    none and no package shipped one (the x64 log said so on every run).
  - `kataglyphis_cli onnx-runtime` loads the chain ORT through
    `ort_runtime::ensure_ort_loaded` and prints its path. The arm64 run job and the
    x64 host step both run it.
  - The GUI's person-detection overlay had never been compiled: `clippy -D warnings`
    found five unused imports and an `if let Some` over a field that is no `Option`
    (`poll_inference`). Both are fixed, and `kataglyphis_telemetry`'s two inference
    counters are `pub`, because the GUI's inference thread calls them. Checked for
    `onnxruntime_directml`, `onnx_tract` and `onnxruntime_cuda`.
  - `Build-Windows.ps1` stages the DLL closure on x64 too, with the hub's
    `Get-ProductDllSearchPath` (the chain ORT, then `C:\runtime\bin`, then the
    target's VC++ runtime). Both arches write `dist\windows-<x64|arm64>`: the
    portable bundle (exe, `resources\`, closure), the MSIX, and the MSI, whose
    generated group now holds every payload DLL.
  - The package features live in `Build-Windows.config.psd1`
    (`gui_windows,onnxruntime_directml`), not in two workflows' env.
- **The Windows x64 lane is a thin caller of the hub's `container-ci-windows.yml`**
  (the family's next sharing step). `scripts/windows/Invoke-WindowsLane.ps1` runs
  the debug tests, the config matrix and `Build-Windows.ps1` in the container. The
  lane's `host-command` runs the renderer tests and the packaged exe on the host.
  The version comes from `version-file: VERSION.txt`, and `dist/windows-x64`
  uploads as one artifact, as the arm64 lane's does. `New-ReleaseArchive.ps1` is
  gone: the bundle is what the zip held, now with its closure and `resources\`.
- **A Windows arm64 lane that cross-builds and then runs the product (owner
  decision 2026-09-25).** `windows-arm64-cross.yml` ("Windows arm64 · cross
  build + run") is a thin caller of ANTfrastructure's reusable
  `container-ci-windows.yml`: `Build-Windows.ps1 -TargetArch arm64` in the
  family image's arm64 bundle, the hub's arch gate over `dist/windows-arm64`
  (every binary arm64, every import resolvable on a clean device), and
  `kataglyphis_cli.exe --help` + `stats` on GitHub's `windows-11-arm`.
  `Build-Windows.ps1` takes `-TargetArch amd64|arm64` (default: the image's
  `WINDOWS_TARGET_ARCH`). A cross build builds with `--target`, runs clippy for
  aarch64, leaves audit/deny, fmt and the tests to the x64 lane, stages the DLL
  closure beside the exe (hub `Copy-PeImportClosure`), and writes the portable
  bundle, `*_arm64.msix` and `*-arm64.msi`. `WindowsCargoTarget.Common` holds
  every arch-dependent path and name, pinned by `CargoTarget.Tests.ps1`; the
  x64 paths are unchanged. The AppxManifest takes its `ProcessorArchitecture`
  from `__MSIX_ARCH__`, and the MSI's generated DLL group is `PayloadDlls`
  (was `OrtRuntime`): the chain ORT on x64, every payload DLL on arm64. Its
  fragment moved out of `dist\msi` into the target directory, and the MSI's
  `InstallerVersion` is 500, which an ARM64 package requires (WIX1143).
  Verified in the arm64 bundle: 10/10 steps, 12 arm64 binaries in the bundle
  (the exe, the VC++ runtime, GStreamer/GLib), the arch gate clean with no
  unresolved import. The hub pin moves from 20bb0026 to 948f7031, which
  carries the reusable lane and `Copy-PeImportClosure`.
- **Three gates this repo did not have.** `--ratchets` on the lint lane (the
  docs cross-reference gate plus eight measurement gates, frozen at
  `<repo>/<gate>.allow`); a PowerShell lint job running ANTfrastructure's
  `Invoke-Lint.ps1 -Path scripts -FailOnAnalyzer`; and
  `scripts/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`, which asserts that
  nothing generated is tracked. Seeded freeze files: `comment-size.allow`,
  `code-complexity.allow`, `dead-functions.allow`.
- **The renderer's three design documents, in `crates/webgpu_renderer/docs/`**:
  `renderer-bounds-invariant.md`, `webgpu-renderer-roadmap.md` and
  `webgpu-gltf-rust-plan.md`. They moved out of `BeschleunigerBallett/docs/`
  under decision D6 — this repo owns the WebGPU renderer, code *and*
  documentation — and BeschleunigerBallett keeps a pointer file at each old
  path rather than a copy. The four pages that describe *both* renderers
  (`gpu-golden-testing.md`, `model-loading.md`, `shader-sharing.md`,
  `webgpu-srgb-audit.md`) stayed there and are still referenced absolutely.
  The four references to the moved pages — `crates/webgpu_renderer/README.md`,
  `src/lib.rs` and twice in `src/render/bounds.rs` — point at the moved pages
  again. Three of them are written `../docs/<name>.md` / `../../docs/<name>.md`,
  relative to the file rather than to the crate: a bare `docs/<name>.md` in code
  is resolved from the REPO ROOT by the docs cross-reference gate, which is what
  it means to a reader too.
- **`scripts/linux/cat-stream/run-producer-pi.sh`**, the Raspberry Pi 5 runner
  for `crates/cat_webrtc`, moved here from OmniAccelerANT under decision D12:
  it belongs with the crate it builds and starts. OmniAccelerANT keeps a
  pointer and still owns the web half, `serve.sh`.

### Changed
- **Every platform lane runs on every push and PR, under the family's new
  workflow names (owner request, 2026-09-24).** `rust_ubuntu26_04.yml` became
  `linux-x64.yml` ("Linux x64 · build + test") and `linux-arm64.yml` ("Linux
  arm64 · build + test"), two thin callers of one `reusable-linux.yml` ("Linux ·
  reusable build", `workflow_call`) that holds every step both architectures
  share; the x64 caller keeps the docs publish and the opt-in feature check.
  `rust_windows2025.yml` became `windows-x64.yml` ("Windows x64 · build +
  test"). The arm64 row no longer needs `[build-arm]` and the Windows lane no
  longer needs `[build-win]` - no build job carries an `if:` - so neither
  reports `skipped` behind a badge that looks like a pass. `workflow_dispatch`
  stays on all three. `lint-gates.yml` is displayed as "Lint gates" and
  `submodule-pins.yml` as "Submodule pins". Each triggered build workflow has
  its own concurrency group (the Windows lane had none); every build job
  carries `timeout-minutes` and a `permissions:` block, and every artifact
  upload `if-no-files-found: error`, which takes this repo's hub
  workflow-conventions census from 9 findings to 0. README badges, AGENTS.md,
  BACKLOG.md, `.github/actionlint.yaml` and the script headers that named the
  old files follow.
- **ONNX Runtime is the family's chain build only, loaded at run time
  (owner rule 2026-09-23).** `ort/download-binaries` is gone from every
  feature: `onnxruntime`, `onnxruntime_directml`, `onnxruntime_cuda`,
  `burn_demos` and `crates/gui`'s `onnxruntime` statically linked pyke's
  prebuilt ORT 1.28.0 from cdn.pyke.io, including the shipped
  `kataglyphis_cli` release. Every ORT feature is `load-dynamic` now
  (`onnxruntime_dynamic` stays as an alias); `Cargo.lock` lost ureq 3, the
  TLS stack and `openssl-sys` with it, nothing was upgraded.
  `crates/inference/src/ort_runtime.rs` picks the dylib — `ORT_DYLIB_PATH`,
  the exe's directory, then the image's chain prefix — and refuses the bare-name
  load that reached Windows ML's `System32\onnxruntime.dll`. It hands `ort` an
  absolute path only (a relative `ORT_DYLIB_PATH` went to the OS search
  verbatim), and refuses any file that does not embed the chain's ORT source
  path (`C:\temp\onnx-src\onnxruntime\core\`, `/opt/onnxruntime/onnxruntime/core/`),
  so a PyPI, GitHub-release or Windows ML copy named by `ORT_DYLIB_PATH` no
  longer loads just because it clears ort's API-24 floor. The CUDA path no
  longer copies provider DLLs out of pyke's download cache. The Windows release
  zip (`scripts/windows/New-ReleaseArchive.ps1`), MSIX and MSI carry the chain
  `onnxruntime.dll` (+ `DirectML.dll`, `onnxruntime_providers_shared.dll`)
  staged from `$env:ONNX_ROOT\bin`. Each package ships a payload that
  ANTfrastructure's ORT census (G6, `Test-OrtProvenanceTree`) proves byte for
  byte against the image's chain ORT, with every importer resolving to it
  (`scripts/windows/modules/WindowsOrtPayload.Common.psm1`); a hub pin before its
  ORT single-source commit of 2026-09-23 stops the build naming it. Whether a
  payload loads ORT is decided over the exe and every DLL it ships, so an
  ORT-consuming DLL beside a plain exe gets the chain ORT and its G6 proof too,
  instead of shipping with nothing beside it for System32's copy. The Linux lane gates
  it with `scripts/linux/check-ort-chain-only.sh` (lock + resolved feature
  graph, all targets, and every `ort` declaration in the graph must carry
  `load-dynamic` — the workspace dependency declares it, so no feature subset
  can link ORT). `run-producer-pi.sh` defaults `ORT_DYLIB_PATH` to
  `/usr/local/lib/onnxruntime-cpu/lib`, not `/opt/opencv5`'s copy.
- **`crates/cat_webrtc` no longer bakes in a path into a sibling submodule
  checkout.** `--image` has no compile-time default; it falls back to
  `$KATAGLYPHIS_CAT_IMAGE`, and with no image and no live-source flag the binary
  exits with a message naming both. The old default pointed out of this
  repository, past its superproject and into another submodule's working tree,
  and resolved nowhere else (decision D5).
- **The Linux CI lane runs `scripts/linux/ci-container-steps.sh <step>`** instead
  of eight inlined copies of the same container prologue.
- **The docs publish and `cancel-in-progress` follow the repository's actual
  default branch** rather than a typed `refs/heads/main`.
- **`AGENTS.md` was rebuilt on ANTfrastructure's six-section template.**
- **`third_party/ANTfrastructure` moved to `604294e2`, and the `powershell-lint`
  job became a `uses:`** — the retirement the job's own comment specified one
  pin earlier ("when a reusable lane carries this gate without a Python build
  attached"). Hub `604294e2` added `build-python-package` to the reusable
  `python-ci-windows.yml` (boolean, `default: true`, so every caller written
  before it is byte-for-byte unaffected), gated
  `build-test-python-package-on-windows` on it, made `GHCR_PAT`
  `required: false` — a required secret is refused at call time and would have
  kept the lint unreachable for exactly the callers it was added for — and made
  the build job assert the token itself, naming `build-python-package: false`
  as the alternative. The call here passes `build-python-package: false`,
  `lint-powershell: true`, `lint-path: scripts` and **no `secrets:` block**;
  nothing was held back from the deleted job. It was compared field by field
  first, not assumed: same `windows-2025` (the lane's `runs-on` default), same
  `timeout-minutes: 20`, the same pinned `actions/checkout@3d3c42e5` (v7.0.1)
  with `submodules: true` and `fetch-depth: 1`, the same
  `Install-Module PSScriptAnalyzer -RequiredVersion 1.25.0 -Force -Scope CurrentUser`,
  and the same `Invoke-Lint.ps1 -Path <lint-path> -FailOnAnalyzer` — where
  `-FailOnAnalyzer` is unconditional upstream and is deliberately not an input,
  which is what this tree measures (`PSSA: 0 error(s), 0 warning(s)
  [PSScriptAnalyzer 1.25.0]` over all 9 files). `shell: pwsh` arrives from the
  lane's workflow-level `defaults` instead of the deleted job-level block. The
  one difference is **additive**: upstream tests for the gate script first and
  throws a message naming the submodule, where the copy here would have
  reported a bare `pwsh -File <absent>` path error. This supersedes the
  "the job stays" decision recorded in the `49be50f0` row below. The
  shared-config templates did not move across this bump either, which
  `sync-shared-config.sh --repo-root . --check` confirms rather than the commit
  range being taken on trust ("Shared config in sync", both rows OK). All three
  gate jobs in `lint-gates.yml` are now one `uses:` onto ANTfrastructure.
- **`third_party/ANTfrastructure` moved to `49be50f0`**, and nothing local was
  deleted for it — deliberately, row by row. The shared-config templates are
  byte-identical across the bump, which
  `sync-shared-config.sh --repo-root . --check` confirms rather than the commit
  range being taken on trust ("Shared config in sync", both rows OK).
  `lint-powershell`/`lint-path` on the hub's reusable `python-ci-windows.yml`
  reproduce this repo's `powershell-lint` job exactly — same runner, same
  pinned PSScriptAnalyzer 1.25.0, same unconditional `-FailOnAnalyzer`, and
  `lint-path` defaults to the `scripts` this job passes — but they are inputs
  to the family's **Python** lane, whose build job has no `if:` and would run a
  Python package build and demand a `GHCR_PAT` alongside the two-minute lint;
  the job stays, with that recorded where it lives.
  `WindowsMediaRuntime.Common` stages a GStreamer/ONNX DLL closure and replaces
  nothing here (the `onnxruntime_*` names in `scripts/windows/` are cargo
  features, not a staging step). `fix_bind_mount_ownership` is a `chown` of a
  container-written tree, not the `git safe.directory` guard
  `ci-container-steps.sh` keeps, so that `BACKLOG.md` row stays blocked.
  `cmake-build.sh --configure-arg` and the bandit argument fix have no caller
  here — no CMakeLists.txt, no bandit. The flatpak pair does not reach
  `package_archive.sh`, which is unchanged, so the tarball-only row stays open.
  All eight "Waiting on ANTfrastructure" rows were re-checked against the new
  pin and say so.
- **`third_party/ANTfrastructure` was pinned at `19286e9f`**, and three local
  forks went with the bump. `scripts/linux/ci-container-steps.sh`'s `fmt-clippy`
  case delegates to `cargo_fmt_clippy.sh` now that the driver takes
  `CARGO_CLIPPY_ARGS` instead of hard-coding `--all-features`; the ~100-line
  MSIX orchestration in `scripts/windows/Build-Windows.ps1` is one
  `Invoke-MsixPackage` call; and both of that script's inline version parses are
  one `Get-PackageVersion`. The vendored `scripts/linux/lib/antfrastructure.sh`
  was re-synced to the rewritten template.
- **Both gate lanes are one `uses:` onto a reusable ANTfrastructure workflow**
  rather than a copied job — `lint-gates.yml` and `submodule-pins.yml`.
- **`.gitmodules` uses an `https://` url**, not `git@github.com:`: an SSH pin is
  inherited by a recursive checkout from OmniAccelerANT, where no key exists.
- **`BACKLOG.md` no longer claims the ARM lane cannot go green.**
  `:latest-cross` is a multi-arch OCI index carrying amd64, arm64 and riscv64 —
  verified 2026-09-15 with `nerdctl manifest inspect`, and AGENTS.md has said so
  since the index landed on 2026-09-04. The lane stays opt-in via `[build-arm]`,
  which is a runner-minutes decision and not a blocker.
- **The Pi runner asks ANTfrastructure for both things it used to retype.**
  The CI image reference comes from `linux/scripts/ci-image-ref.sh` (a literal
  in a tracked `*.sh` is what the CI-image-ref lint gate refuses), and the
  loader path is composed by the image's own `media-env.sh` plus
  `path-helpers.sh`'s `_path_prepend_unique` instead of a hand-typed list that
  had `/opt/gcc-16.2.0` frozen into it.

### Fixed
- **The ORT payload suite follows the hub's stricter G6 fingerprint (2026-09-25).**
  Since hub e72a9a37, the census counts a source path as an ORT fingerprint only when it
  is a whole `__FILE__` string ending in NUL. The hub bump to 20bb0026, made for the
  Windows sccache fix, brought that rule here. `OrtPayload.Tests.ps1`'s fake chain and
  stale DLLs wrote the path followed by a space, so the stale case read as `UNPROVEN`,
  not `STALE`, and the lint lane's Pester step failed (run 36101778073). The fixtures
  now end the path in NUL, as OmniAccelerANT's `OrtRunner.Tests.ps1` does: 10 of 10 pass.
- **The `gui_windows` build compiles again (2026-09-24).** The wgpu 30 upgrade set
  the new `SurfaceConfiguration::color_space` in the renderer but missed the second
  literal in `crates/gui/src/gui_wgpu/wgpu_init.rs`. That module compiles only under
  `gui_windows`, so no Linux build had ever seen it. The Windows lane stopped there
  with `E0063` (run 36044802271). It now uses `SurfaceColorSpace::Auto` like the
  renderer. Checked in the Linux image with `cargo check --locked`: the root crate
  with `gui_windows`, plus all four GUI rows of the Windows config matrix built as
  `kataglyphis_cli`. Without the fix the same check reproduces the `E0063`. The
  opt-in `feature-matrix` job gains a `gui_windows` row for that reason, and the
  "`gui_linux` is the wgpu path" claim is corrected in the job's comment and in
  AGENTS.md: that feature pulls the wgpu dependencies but compiles no GUI module.
- **The WebGPU renderer compiles for wasm32 again (2026-09-24).** No published
  `egui-winit` builds for `wasm32-unknown-unknown`: on wasm32, egui's `DroppedFile`
  requires `bytes_async`, and `egui-winit`'s `NativeFile` implements only `bytes`
  (emilk/egui#8436). The renderer's wasm demo drives its overlay through
  `egui_winit::State`, so BeschleunigerBallett's wasm size-budget gate, which builds
  it for wasm32, failed (run 36020443791). Upstream fixed it in #8516, merged to
  egui's main on 09-07 but not in 0.36.2. `third_party/egui-winit-0.36.2` is the
  published crate (sha256 checked against the crates.io index) with #8516's `lib.rs`
  change applied unchanged. The root `Cargo.toml` routes `egui-winit` to it through
  `[patch.crates-io]`, and the egui family moves to 0.36.2 (the patched crate needs
  it). Checked in the Linux image: `cargo check --locked` for wasm32 and native, and
  the gate's own `cargo build --release` for wasm32 (10,241,933 bytes before
  wasm-opt, budget 12 MiB after). Temporary: `PATCHED.md` and a blocked BACKLOG
  item say when and how to remove it.
- **The host run of the renderer's lib tests finds their fixtures (2026-09-24).** The
  first host run (36038436509) started the binary and ran 163 tests: 159 passed and
  4 failed, every one on a path. The tests embed `env!("CARGO_MANIFEST_DIR")` at compile
  time, which in the container is `C:\ws\crates\webgpu_renderer`. The runner's checkout
  is `D:\ws`, so `tests\assets\*.gltf` and the crate's `src` tree were not found.
  `Invoke-HostTests.ps1 -ContainerRoot 'C:\ws'` now links that path to the workspace
  with a directory junction (no admin needed). It refuses an existing `C:\ws` that is
  not a junction to this tree. Tested in a throwaway tree: create and read through the
  junction, accept a matching one, refuse a real directory.
- **The Windows lane runs the WebGPU renderer's lib tests on the runner host
  (2026-09-24).** Its first always-on run (36019995362) built everything and
  then failed `Invoke-DebugTests.ps1`'s renderer step: the lib test binary
  exited `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`, like every
  renderer test binary. wgpu's `gles` backend imports `opengl32.dll` at load
  time, and servercore does not ship it (README, *Tests*). The container now
  only builds that binary (`cargo test --no-run --message-format=json`) and
  records its path in `target\host-tests\`. A new host step,
  `Invoke-HostTests.ps1`, runs it from the same bind-mounted tree. It refuses
  a missing list, an empty list and a missing binary. Tested against cargo's
  message shapes under strict mode, and against those failure cases.
- **A pull request no longer reds the Linux lane at its artifact upload
  (2026-09-24).** The artifact was named after `github.ref_name`, which on a PR
  is `<number>/merge`, and `upload-artifact` refuses a `/` in a name - run
  35752031200 (2026-09-22) built, tested and packaged green and then failed
  there. It is named after `VERSION` now, as the Windows lane's artifacts and
  the tarball already were. Found while making the lane run on every PR on
  both architectures.
- **The `TEXTURE_2D_SHAPE_OK` regression guard now matches the code rustfmt
  actually produces.** `texture_desc_single_definition` demanded the marker
  comment on the *same line* as the `wgpu::TextureDescriptor {` it exempts, but
  that line runs past `max_width` at all three non-goal sites, so rustfmt 1.9.0
  (toolchain 1.98.1) moves the comment down into the literal's body and puts it
  back there on every `cargo fmt`. The two gates contradicted each other: no
  source text could satisfy both. The guard now accepts the marker on the
  literal's own line *or* as the first line of its body. Every assertion is
  unchanged - an unmarked literal still fails, the marked count is still exactly
  3, and `render/texture.rs` still holds exactly 1 definition. Latent since
  2026-08-07, when the graphics-stack upgrade reflowed the three comments; it
  surfaced only now, because until the security gate above went green the lane
  failed at `cargo audit` and never reached `cargo test`.
- **The security gate is green again**, entirely by upgrading rather than by
  ignoring: `h2` 0.4.15 → 0.4.19 (RUSTSEC-2026-0258), `chacha20` 0.10.1 → 0.10.2
  (yanked), `stable-vec` 0.4.2 → 0.4.3 (unsound) and `rustls` 0.23.43 → 0.23.45
  with `rustls-webpki` 0.103.13 → 0.103.15 (RUSTSEC-2026-0285). No RUSTSEC id
  was added to either ignore list; two stale ones were removed from both
  (quick-xml RUSTSEC-2026-0194 / -0195, fixed by quick-xml 0.41.0).
- Formatting and clippy against the pinned toolchain 1.98.1: two rustfmt hunks,
  and clippy 1.98's new `chunks_exact_to_as_chunks` allowed once at the
  workspace level with a reason (see `BACKLOG.md`).
- `scripts/windows/Build-Windows.ps1` installs cargo-audit and cargo-deny
  `--version`-pinned from `versions.env`, and no longer swallows the install
  failure or prepends a scoop shims directory that does not exist in the image.

### Removed
- 15 committed Windows build logs under `logs/windows/` (untracked, kept on
  disk), the empty README *Prerequisites*, *Roadmap* and *Acknowledgements*
  sections, and the Lorem-ipsum feature table.

## 2026-08-07

### Verified baselines (container, 32 CPUs)

**2026-08-07, winamd64, rustc 1.97.1** — `Invoke-StevedoreBuild.ps1 -MemoryGb 32`:

- Builds: debug 1m35s, profile 1m32s, release 1m12s — all three green. Release binary verified on the host: `stats --path README.md` → `Lines: 476, Words: 1905, Bytes: 20104`. (Those figures describe the README **as it stood that day**; the file has since grown — `wc -lwc README.md` measured 537/2512/24364 on 2026-09-06 — so a re-run printing bigger numbers is the tool working on a bigger file, not a regression. Compare a re-run against the README of the same date, not against this line.)
- Tests: the 8 that predate `crates/webgpu_renderer` still pass (3 integration, 1 proptest, 4 telemetry). **`kataglyphis_webgpu_renderer` is excluded from the container run** — `scripts/windows/container/Test-RustAll.ps1` passes `--exclude kataglyphis_webgpu_renderer` and logs that it did. Its test binaries exit `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`, because linking wgpu with the `gles` backend makes the executable import `opengl32.dll`, which Server Core does not ship. The loader resolves that import, so no runtime flag helps; without the exclusion the whole `cargo test --workspace` crashed and reported nothing. `gles` stays on purpose (OpenGL fallback for hosts without Vulkan/DX12) — run `cargo test -p kataglyphis_webgpu_renderer --locked` on a desktop Windows machine instead.

  Not a regression from the wgpu 30 upgrade. The old "8 passed / 0 failed" baseline was recorded on 2026-07-17, and the renderer crate landed on 2026-07-18 — the container test lane has therefore *never* run with that crate present. The image is Server Core with no GPU stack; a wgpu-linked binary needs graphics DLLs it does not ship.

  `Invoke-StevedoreBuild.ps1 -Test` used to fail as a whole because of this. It no longer does — the exclusion lives in `Test-RustAll.ps1`, so the container lane is green and reports 8 passed. Drop the `--exclude` once the image carries the missing DLLs.

**2026-07-17** (superseded, kept because it is what the 8-test figure refers to): builds debug 1m11s / profile 1m31s / release 1m08s; tests 8 passed / 0 failed, 1 doc-test ignored — measured before `crates/webgpu_renderer` existed.

### Packaging: what actually happens when you run it

Verified 2026-08-07 by running `Build-Windows.ps1 -SkipTests` in `:winamd64`:

- **MSIX works.** `Kataglyphis.RustProjectTemplate_2.3.4.0_x64.msix`, 51.69 MB, manifest with every token substituted. `makeappx.exe` resolves via `Resolve-WindowsSdkToolPath` to `Windows Kits\10\bin\10.0.26100.0\x64\`. The identity became `Kataglyphis.OxidANT` on 2026-09-05, so a build today writes `Kataglyphis.OxidANT_<VERSION>_x64.msix`; the old filename stands here because it is what that run actually produced. Windows treats the two identities as different apps, so an installation predating that date is not upgraded — it has to be uninstalled first, see the MSIX section of the README.
- **MSI works, but only since the WiX v4 migration** (2026-08-07). It had never produced a file. Two independent faults, both masked by the step being optional:
  1. `cargo wix -p kataglyphis_cli` looks for WXS files inside the package it was pointed at (`crates/cli/wix/`); this repo keeps its single WiX source at the workspace root. `Msi.WxsFile` had been sitting unread in the config the whole time.
  2. Even with the path fixed, **cargo-wix cannot drive this image.** 0.3.9 is its newest release and it shells out to WiX v3's `candle.exe`/`light.exe`. ANTfrastructure installs **WiX 4.0.6** as a dotnet tool — a single `wix.exe`, no candle — so it failed with *"The compiler application ('candle') does not exist at the 'C:\WiX' path"*.

  `Build-Windows.ps1` now calls `wix.exe build` directly (resolved from `$env:WIX`, then PATH) and `wix/main.wxs` is **WiX v4 schema**: `<Package>` instead of `<Product>` + inner `<Package>`, `<SummaryInformation>`, `<StandardDirectory>` instead of the `TARGETDIR` nesting, `Bitness='always64'` for `Win64='yes'`, `AllowAbsent` for `Absent`, and `<ui:WixUI>` for `<UIRef>`. `WixUI_FeatureTree` needs `-ext WixToolset.UI.wixext`, which the image already ships (4.0.4). Paths that move with the build — the binary follows `CARGO_TARGET_DIR` — go in as `-d Version= / ExeSource= / LicenseRtf=` preprocessor variables, so the WXS never assumes a `target\release` beside the workspace root.

  **If you touch this: cargo-wix is not an option again unless it gains WiX 4 support.** Check its releases before reintroducing it.

**Packaging and security steps are now `Invoke-BuildStep -Critical`, not `Invoke-BuildOptional`.** That matters because ANTfrastructure's `Invoke-BuildOptional` is `try { & $Script } catch { Write-BuildLogWarning }` and **never registers the step with the build context** — so it cannot appear in the summary at all. The pre-fix run reported **"7 steps, 7 succeeded, 0 failed (100% success rate)"** while MSI *and* the license check had failed. If you see a suspiciously perfect summary, that percentage covers only the `Invoke-BuildStep` steps; read the WARNING lines.

`cargo-deny licenses` also failed on that run (advisories, bans and sources passed) and was equally invisible. Fixed by allowing `BSL-1.0` in `deny.toml` — xxhash-rust via cubecl-common → burn; it was the only rejection.

`CARGO_TARGET_DIR` may be absolute — the in-container scripts set `C:\ct`. `Build-Windows.ps1` now handles that (`IsPathRooted`); before, `Join-Path` produced `C:\...\workspace\C:\ct\msix-staging` and MSIX died on "The filename, directory name, or volume label syntax is incorrect".

### The graphics-stack upgrade

`wgpu` 29→30, `naga` 29→30, `egui`/`egui-wgpu`/`egui-winit` 0.35→0.36, `glam` 0.30→0.33 and `pollster` 0.4→1.0 moved as one coupled set (`egui-wgpu` 0.35 pins `wgpu ^29`, 0.36 pins `^30`, so none of them could move alone). What changed, so the next person does not have to rediscover it:

- **`VertexState::buffers` is `&[Option<VertexBufferLayout>]`** — a slot can now be left unbound without shifting the ones after it.
- **`BufferSlice::get_mapped_range` returns `Result`.** Seven call sites. Only `render_to_pixels_with_format` returns `Result` and propagates; the other six are in functions whose caller has already awaited the map, so they `expect` with a message naming that invariant.
- **Presentation moved from `SurfaceTexture::present(self)` to `Queue::present(&self, texture)`.** Three sites, two of which the default Linux build never compiles (one is `cfg(wasm32)`, one is behind a GUI feature) — check them by hand or with the `feature-matrix` job.
- **`RequestAdapterOptions::apply_limit_buckets`** (new, no default): rounds reported adapter limits to coarse presets so a host exposing wgpu to *untrusted* content cannot fingerprint the machine. This renderer is the trusted application, so it is `false` — real limits, as wgpu 29 had.
- **`SurfaceConfiguration::color_space`** (new, no default): set to `SurfaceColorSpace::Auto`, the type's own default and the only value guaranteed supported for every format in `SurfaceCapabilities::formats`. Anything else (an HDR space) needs a capability check first.
- **glam 0.33 moved the camera constructors off `Mat4`** and split them by clip-space convention: `opengl` (NDC Z −1..1), `directx` (Z 0..1, Y up), `vulkan` (Z 0..1, Y down). **`directx` is the one that matches** — it reproduces the old `Mat4::perspective_rh`/`orthographic_rh`/`perspective_infinite_rh` bit for bit. That was verified by compiling both against glam 0.33.3 and comparing the matrices, not inferred from the names: the `vulkan` module is Y-**down**, and picking it would have flipped the image with no compile error. The old methods are deprecated but still present, so `-D warnings` is what forces the migration.

Guard this with the golden tests, not with the compiler: a clip-space or Y-axis mistake compiles perfectly and only shows up in pixels. See the GPU note above for how to make them actually run.

The upgrade was verified rendering-neutral: 333 tests pass, the only failure is the pre-existing instance-normal bug, and it still reports **exactly 987 differing pixels** — the same figure as before the upgrade. That number is the useful signal here; a changed clip-space or flipped Y would have moved it.

### The tract 0.22 → 0.23 migration

Dependabot offered this as `build(deps): bump tract-onnx from 0.22.3 to 0.23.4`. It is **not** a drop-in bump — it breaks in four separate ways, none of which the PR title suggests, and only `crates/inference/src/person_detection/{mod,tract_backend}.rs` are affected (the `onnx_tract` feature is off by default, so nothing else notices).

- **`SimplePlan` is gone from the prelude.** It was renamed to `RunnableModel`; the alias to use is `TypedRunnableModel`, and it is **fully applied** — `pub type TypedRunnableModel = SimplePlan<TypedFact, Box<dyn TypedOp>>`. Passing it a generic argument (the old third `TypedModel` parameter) fails with *"type alias takes 0 generic arguments but 1 generic argument was supplied"*.
- **`run` takes `self: &Arc<Self>`.** A `Box<TractPlan>` does not resolve the method at all — the error is a bare *"no method named `run`"*, which reads like a missing trait import and is not.
- **`into_runnable()` already returns an `Arc`.** So the Arc is neither ours to add nor to strip; `load_tract_model` returns `Arc<TractPlan>` and the `Backend::Tract` variant stores it directly.
- **`Tensor::as_slice` was removed.** The safe replacement is `to_plain_array_view::<f32>()`, which errors unless the storage is plain *and* the datum type matches — the same two conditions the old call checked. rustc's *"there is a method `slice` with a similar name"* suggestion points somewhere else entirely; do not follow it.

The lockfile also gains `tract-extra`, `tract-pulse`, `tract-pulse-opl`, `tract-transformers` and `typeid`. `cargo deny check licenses` passes with them (verified, exit 0) — no new `deny.toml` allowances were needed.
