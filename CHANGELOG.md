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

### Fixed
- The shared riscv64 cross environment isolates GStreamer pkg-config from the amd64 image's
  metadata, and its smoke rejects x86_64 link paths before the workspace tests run.
- The README's shader guide uses a GitHub permalink, so its link also resolves
  when the README is included as the crate's rustdoc front page.
- Shared shader comments now point to OxidANT's color-space guide; the migrated
  sources no longer refer to a documentation path found only in BeschleunigerBallett.

### Changed
- **Shared contracts and standalone shaders** (2026-10-22): preprocessing runs the
  ModelANT RGB/RGBA, stride, offset and letterbox references; each concurrent request
  retains its model even if another path replaces the bounded cache. Texture ABI types,
  result codes and header are generated from one contract, and loaded plugins must
  report the expected version before Rust resolves their frame callback.
- **GTK4 builds on Linux** (2026-10-22): `gui-gtk` uses the image's GTK4 >= 4.10,
  keeps its bus watch through the session and stops video on shutdown. Linux tests
  require its lifecycle check under Xvfb; packages include GTK's closure and video sink.
- **The README header shows OxidANT's own logo, `images/logo.svg`** (2026-10-21, owner): a rusted
  ant inside a gear, from the family set DocumANTation's `scripts/make_repo_logos.py` draws.
  `images/logo.png`, the package icon and the rustdoc logo, is rendered from it at 256 px.
- **The docs site carries the guides** (2026-10-21, ANTfrastructure's rustdoc builder takes `guides`
  since that day). `docs.yml` passes all eight `docs/` pages; each is rendered in rustdoc's layout
  beside `oxidant/index.html` and listed atop every page's sidebar, and a link into the repository
  leads to GitHub. Proof in `:latest`: 8 guides, 478 of 594 pages navigated, the sidebar check green
  on the crate page and every guide at 1400, 700 and 400 px, the link gate 595 documents with its 24
  frozen findings.
- **The cxx bridge is its own crate, `crates/cxx_bridge`** (`kataglyphis_cxx_bridge`, 2026-10-21).
  BeschleunigerBallett's engine calls one function, `rusty_cxxbridge_integer`, and took it from the
  root staticlib, which since 3.0.0 links GStreamer and the ORT loader on Linux and Windows. The new
  crate depends on `cxx` alone (native-static-libs on Linux: `-lstdc++ -lgcc_s -lutil -lrt -lpthread
  -lm -ldl -lc`), and BeschleunigerBallett imports it with `CRATES kataglyphis_cxx_bridge`. The root
  package loses `src/native_only.rs`, `cxx` and `cxx-build`; its `build.rs` only sets `cfg(desktop)`.
  **Breaking for a `corrosion_add_cxxbridge` over `oxidant`'s `native_only.rs`**: BeschleunigerBallett
  was the only one and moves with its gitlink.
- **3.0.0: no Cargo features any more; every target builds what it can run** (2026-10-20, owner:
  "They are no longer just an opt-in feature. They will stay there forever"). Every `[features]`
  table is gone, each dependency is unconditional or `[target.'cfg(...)'.dependencies]`, and the code
  gates on `cfg(target_os)` and the root's `cfg(desktop)`. **Breaking for anyone passing
  `--features`**: cargo now refuses the names, OmniAccelerANT's `KATAGLYPHIS_RUST_FEATURES` included.
  - Linux (x64, arm64, riscv64) and Windows (x64, arm64) build the whole workspace: tract and the chain
    ORT with CUDA registered (DirectML on Windows), GStreamer capture, the webcam engine, the ESP32
    bridge, the wgpu GUI and the burn demos. `aarch64-linux-android` builds `oxidant` with the
    detector on tract. `wasm32-unknown-unknown` builds it without the detector: tract built there and
    took the release wasm from 0.66 to 15.5 MB, but the model loads from a file and the web has no file
    system. The calls a target cannot serve answer so; no frb signature changed. The GTK4 GUI waits
    for GTK4 in the Linux image (`BACKLOG.md` has the change).
  - ORT first, tract as the fallback, loudly: with `KATAGLYPHIS_ONNX_BACKEND` unset the detector warns
    `Falling back to tract: <ORT's reason>`, and a model neither loads names both reasons.
    `PersonDetector::with_backend` and `backend_name` are new. `KATAGLYPHIS_ORT_DEVICE=auto` on an
    ORT without the CUDA provider now runs ORT on the CPU, where it dropped to tract.
    `model_errors.rs`, which compiled out when both backends were on, tests each backend's error and
    the pair's; `real_model.rs` checks that ORT is the backend and that tract finds both cats too;
    `ort_fallback.rs` (new) points `ORT_DYLIB_PATH` at nothing and checks the warning and tract's result.
  - The burn demos are their own crate, `crates/burn_demos` (`kataglyphis_burn_demos`, bin
    `burn_demos`, no longer `burn-demos`: a hyphen in `CARGO_BIN_EXE_*` does not survive the hub's sh
    sccache launcher), so `oxidant`, which both consumers link, carries no Burn; neither does it depend
    on `kataglyphis_gui` any more. The demos return their results and have 19 tests (each demo, the
    binary, the detector demo on the real model), which found `binary_cross_entropy` using `1 - pred`
    where `1 - target` belongs. Plots use plotters' `ab_glyph` and the system's TrueType font
    (`fc-match`, then the usual files; Lucida Console in Server Core) instead of font-kit, whose
    pkg-config link paths put the distro's `libstdc++` ahead of GCC 16.2's under tarpaulin, where the
    chain ORT then failed to `dlopen`.
  - The GUI's wgpu has no GL backend, which its `GpuBackend` never picks: the `opengl32.dll` import
    kept the CLI from loading in Server Core. The GUI crate, untested until now, has 15 tests, and the
    CLI's integration tests run `media-check`.
  - `kataglyphis_media::ensure_gst_initialized` sets `GST_PLUGIN_SYSTEM_PATH(_1_0)=""`,
    `GST_REGISTRY_FORK=no` and a per-copy `GST_REGISTRY` beside the bundled `GST_PLUGIN_PATH`, each
    only when unset (the hub's rule for a package's GStreamer).
  - **The Linux packages carry their closure**: `scripts/linux/package-linux.sh` replaces the hub's
    one-binary `package_archive.sh` call; the bundle has the image's GStreamer, GLib, FFmpeg and GCC
    runtime (`elf-closure.sh`), the chain ORT unpatched, the GUI's plugins, the model and the image's
    glibc for an older host (`bundle-loader.sh --beside`), and is proven before packing. The new
    `clean-host` job installs and starts the `.deb`, the tarball and the AppImage on a bare runner.
  - CI: `TEST_FEATURES`, `FEATURE_CHECK_SETS` and the `feature-check` step, `linux-x64.yml`'s
    `feature-matrix` job and its `feature-checks-per-job` input, `Invoke-WindowsConfigMatrix.ps1`
    (a featureless `base` and four feature builds; `Invoke-AppProfiles.ps1` now builds and runs the
    full CLI in debug and release, since its `profile` build ran the 16 GB runner out of LLVM memory), `Build.CargoFeatures`/`CARGO_FEATURES` and every `--features` in the
    Windows scripts are gone, and so is `deny.toml`'s `all-features`. Coverage now measures
    `crates/media`, `src/webcam_engine.rs`, the GUI and the burn demos, which no module hides behind
    a feature any more: 70.98 % of 12357 lines (72.9 % of fewer lines on 2026-10-19), with new floors for
    the three crates and `src` raised to 19 %.
  - Proof, 2026-10-20: in `:latest` every container step of the Linux lane (720 tests passed, the
    bench, release, docs and the packages with their closure, G6 and element checks), the riscv64 step
    under QEMU (720 passed, 8 min 33 s), `oxidant` for `aarch64-linux-android` (the NDK's clang) and
    `wasm32-unknown-unknown` (the web lane's nightly and flags) under `clippy -D warnings`, and the
    packages on a bare `ubuntu:26.04` and on `debian:13` (glibc 2.41, so the bundled glibc); in
    `:winamd64` the x64 lane's container half (381 tests, the CLI in each profile, `Build-Windows.ps1`'s
    14 steps) and the bundle's `onnx-runtime` and `media-check` on the host.
- **Dependabot is off** (2026-10-20, owner: dependency upgrades go only through Renovate as a local
  CLI, ANTfrastructure's `docs/dependency-updates.md`). `.github/dependabot.yml` (cargo and GitHub
  Actions) is deleted and the repository's Dependabot security-update PRs are disabled; Dependabot
  alerts stay on. `renovate.json` and `docs/updates.md` no longer call it present.
- **The README logo is the family's ant** (2026-10-17, owner: one icon in every repository),
  drawn from OmniAccelerANT's `assets/icons/kataglyphis_app_icon.svg`.
- **The README's hero image is an animated WebP** (2026-10-17): `images/Rust.gif` was 7.5 MiB, the same
  136 frames as `images/Rust.webp` (ffmpeg's libwebp, quality 70) are 2.1 MiB, with no visible loss
  side by side. A checkout and every README view load 5.4 MiB less.
- **The README is a short overview** (2026-10-19, owner: the family's README rule, ANTfrastructure's
  `docs/repository-family.md`). 671 lines became 83; the rest moved to eight new guides in `docs/`
  (`getting-started`, `crates`, `testing`, `running`, `observant`, `windows`, `packaging`, `updates`),
  and the contributing, licence and contact boilerplate to the family page. Its links are absolute,
  since the README is also the `oxidant` crate's rustdoc front page. Corrected on the way: the test
  and clippy commands take the lanes' four `TEST_FEATURES` and the `KATAGLYPHIS_REQUIRE_*` trio,
  ObservANT's default config is `observant.toml`, zune-core is no longer held at 0.5.1, the image's
  WiX is 7, Renovate's `--apply` is no longer gitlinks only, and the crate table lists the two ESP32
  crates.
- **The release runs on the hub's `release-publish.yml`** (2026-10-19, ANTfrastructure c3fc7585):
  `release.yml`'s version check and release job are its two stages, and the hub's
  `collect-release-assets.sh` picks the files by `release.yml`'s `collect:` options.
  `scripts/linux/collect-release-assets.sh` and its suite are gone; the hub's suite holds OxidANT's
  uploads. On one fake release run in `:latest`, the old and the new collector gave the same 14
  files, byte for byte, the zips' members included.
- **`run-producer-pi.sh` collects the host libcamera closure with the hub's `elf-closure.sh`**
  (2026-10-19): the four roots go in under their SONAMEs, then `--ldd --no-default-system
  --no-runpath` with the same eight C-runtime names as `--system`. In `:latest`, over the image's
  libcamera, libcamera-base and two IPAs (25 libraries) and over `libgstlibcamera.so` plus the chain
  ORT (36), the old walk and the new one hold the same SONAMEs with the same bytes; only the old
  walk's real-name copies beside its soname links (`libcamera.so.0.7.2`) are gone. A library ldd
  cannot find now stops the collection instead of being left out.
- **The script suites run on the hub's `lib/test-harness.sh`** (2026-10-19):
  `test-check-crate-coverage.sh` (8 assertions) and `test-bench-report.sh` (12) drop their own
  `expect`/`check` counters for `t_assert_rc`, `t_assert_grep` and `t_summary`, and now report every
  failed assertion instead of stopping at the first. `lint-gates.yml`'s `script-tests` job checks the
  hub submodule out for it. In `:latest` both pass with the old counts, and four planted bugs in
  `check-crate-coverage.py` and `bench-report.py` fail them.
- **`fetch-models.sh` and `Get-Models.ps1` run the hub's `fetch-data-lock.py`** (2026-10-19,
  ANTfrastructure CON98): `python3 -I` and `python -I` on `resources/models/models.lock`, the
  callers' `--dest`/`-Destination` and `--lock`/`-Lock` passed on. Windows now keeps a cache too
  (`%LOCALAPPDATA%\kataglyphis-models`), and the cache's old bare `<sha256>` entries still serve.
  In `:latest` the old and the new `fetch-models.sh` wrote the same three files, `0644`, byte for
  byte; in `:winamd64`, where `python` is `C:\temp\cpython\PCbuild\amd64\python.exe` (3.15.0) and
  `python3` is not on `PATH`, the old and the new `Get-Models.ps1` did too.
- **The generated-artifacts check is the hub's suite** (2026-10-19, ANTfrastructure 1a174d9c).
  `scripts/windows/tests/Repo.GeneratedArtifacts.Tests.ps1` is gone; `lint-gates.yml`'s
  `generated-artifacts` job runs the hub's `shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`,
  which reads the same ten pathspecs from `scripts/windows/generated-artifacts.txt`, then this repo's
  own suites in a second step. Under Pester 3.4.0 and 6.1.0 it passes 2/2 on this tree; a `docs/*`
  line fails it on 9 tracked files, and a `**/__pycache__/` line is refused.
- **`Build-Windows.ps1` reads the MSIX version with the hub's `Get-PackageVersion -VersionFile
  'Cargo.toml' -Required`** (2026-10-19, ANTfrastructure 1a174d9c). `Get-CargoMsixVersion` and its
  four cases in `CargoTarget.Tests.ps1` are gone; the hub's reader takes `[workspace.package]` before
  `[package]` the same way and throws on a `Cargo.toml` without a version. This tree gives `2.3.4.0`
  (MSI `2.3.4`) as before, and `MSIX_VERSION` still overrides it.
- **`Invoke-WindowsLane.ps1` moves the CI compiler cache with the hub's `Restore-CiCompilerCache
  -Cache sccache, cargo` and `Save-CiCompilerCache`** (2026-10-19, ANTfrastructure 1a174d9c), in place
  of its own robocopy pair; it still sets `RUSTC_WRAPPER` and the `SCCACHE_ERROR_LOG` directory.
  In `:winamd64` with stub lane steps, the old and the new script leave the same trees, a failing
  step included: sccache and `registry\cache` back in `CI_COMPILER_CACHE`, `registry\src` and
  `git\checkouts` left behind. A cache that cannot move back now warns instead of failing the lane.
- **`Invoke-HostTests.ps1` grades with `Invoke-StagedTests.ps1 -AssertVerdict`** (2026-10-19,
  ANTfrastructure 1a174d9c) instead of parsing the `TESTS:` line itself. On the host and in
  `:winamd64`, over synthetic manifests, the old and the new script give the same exit codes and
  counts for a pass, a failure, a run whose one test skipped itself and a missing binary; only the
  messages say "staged test(s)" where they said "host test(s)".
- **`Invoke-StevedoreBuild.ps1` runs on the hub's `Invoke-RepoContainerBuild.ps1`** (2026-10-19,
  ANTfrastructure 1a174d9c). The driver keeps only its spec; the hub streams the sources into a
  reused `kata-rust-build` container by tar-pipe and `target\container\<profile>` and `logs` back,
  and checks that every exe built there reached the host. `Build-RustAll.ps1` and `Test-RustAll.ps1`
  take the runner's `-Workspace` and import `WindowsContainerLog.Common` through
  `Resolve-BuildModule.ps1`, where they read `C:\ws-mnt` and `C:\host-scratch`. Gone: the bind
  mounts, the `%LOCALAPPDATA%` scratch dir, `-StageSources`, `-StagingDir` and `-Docker` (now
  `-DockerExe`); new: `-FreshContainer` and `-WhatIf`. `-Test` in `:winamd64` (rustc 1.99.0, a new
  container): debug 1m46s, profile 1m50s, release 1m15s, 325 tests passed in 25 binaries, 6m09s in
  all; 2 exes per profile verified on the host, and `release\kataglyphis_cli.exe stats` runs there. A
  second build-only run in the kept container took 12 s.
- **AGENTS.md and the shipped texts say what is true** (2026-10-19). AGENTS.md: Renovate's
  `--apply` writes every ecosystem, not gitlinks only; the zune-core hold is gone (`Cargo.lock`
  has 0.5.3; its history is in `docs/updates.md`); the clippy, test and bench commands take the
  lanes' four `TEST_FEATURES`; the image's `CARGO_HOME` belongs to uid 1001, so the feature job
  drops its `-e CARGO_HOME`; the crate list names the two ESP32 crates. ObservANT's `--help`,
  its config and COCO comments and the Pi runner's message no longer say YOLO (the model is
  DEIM-D-FINE-N). The root package's description, the CLI's `about` and the MSI and MSIX
  descriptions no longer call it a project template. `docs/crates.md`'s literature keeps the
  renderer's shading references and drops the Vulkan, C++ and path-tracing ones.
- **The docs link the hub instead of restating it** (2026-10-19, ANTfrastructure a447ef3c).
  AGENTS.md's CI rule, workflow names and concurrency, https submodule rule, CRLF fix, riscv64
  mechanism and QEMU traps and history rewrite point at the hub's pages, keeping this repo's
  tokens, release contents and exceptions. `docs/getting-started.md`'s container recipe is the
  one measured that day (PowerShell `nerdctl`, `target/` and the cargo home on named volumes,
  the uid 1001 chown), not a copy of the hub's; `docs/updates.md` keeps this repo's Renovate
  facts. `doc-dupes-hub.allow` turns on the hub's `doc-dupes --against-hub` gate, with the
  owner's docs rule as its one row. Corrected: `[ci linux]` runs the riscv64 lane too, and
  `.github/renovate.json` says `branch = develop`.
- **The docs site has its own workflow** (2026-10-19, ANTfrastructure 89b098c9). `docs.yml` calls
  the hub's `build-docs.yml` with `builder: rustdoc`; the Linux lanes lose the `docs` input, the
  *Create docs* and FTP steps and the `SERVER`/`USERNAME`/`PW` secrets, which `docs.yml` maps onto
  the hub's `FTP_*`. The site documents all 11 crates (the root package alone before), carries the
  hub's `.htaccess` and 404 page, and fails on a dead internal link; `docs/site-links.allow`
  freezes the 20 that rustdoc copies from `tracing`'s blanket `WithSubscriber` docs. It publishes
  on a push to `develop` or `main`, never cancelled, where the lane published from its last step
  with `continue-on-error: true` and not at all after a red step. The site job in `:latest`: 518
  documents, 20 findings, 20 frozen, `.htaccess` and `404.html` in `target/doc`.
- **`resources/web/combined-theme.css` is no longer tracked** (2026-10-19). ANTfrastructure's
  `cargo_build_doc.sh` writes it from the family theme and `rustdoc-mapping.css` on every docs build
  and is its only reader (`--extend-css`), so every theme move left a diff in the checkout (558
  lines in, 114 out after the move to 89b098c9). It is in `.gitignore` now; the Windows bundle, MSIX
  and MSI, which copy `resources/` whole, no longer ship it.
- **AGENTS.md drops "The image's Rust may be older than its own pin"** (2026-10-19). In `:latest`
  (`sha256:b52f8c34…`, the registry's current tag) `rustc` is `/usr/local/cargo/bin`'s 1.99.0 in login
  and non-login shells, the hub's `RUST_VERSION=1.99.0`; Ubuntu's 1.93.1 in `/usr/bin` sits behind it,
  and the hub's cargo drivers hoist the pinned toolchain and log it (`rust toolchain: … rustc 1.99.0`).
  The section is now *The trap a Windows checkout hits before anything runs*, the CRLF one.

### Fixed
- **Every Windows build compiles C and C++ with clang-cl** (2026-10-21). The arm64 cross lane and the
  local Stevedore driver (`Build-RustAll.ps1`, `Test-RustAll.ps1`) left `CC` and `CXX` unset, so
  cc-rs took MSVC's `cl`; they set both now, as the x64 lane does. cc-rs 1.8 adds the triple itself
  (`--target=aarch64-pc-windows-msvc`) and archives with `llvm-lib`.
- **The Windows real-model tests run instead of passing by skipping** (2026-10-21). Neither lane
  fetched the model before its tests, and `Build-Windows.ps1` ran *Unit Tests* before *Fetch
  Models*. `Invoke-DebugTests.ps1`, `Test-RustAll.ps1` and *Unit Tests* fetch it first and set
  `KATAGLYPHIS_REQUIRE_MODEL=1`; `Stage-CrossTests.ps1` stages the model and the chain ORT for the
  arm64 device, whose test command requires the model too, so there the detector also proves the
  DirectML fallback to the CPU.
- **The coverage figure leaves out the vendored crates** (2026-10-21). tarpaulin counts every file
  under the workspace, so the vendored `third_party/tract-linalg-0.23.8` took the Linux lanes'
  figure to 56.36 % against the 70 % floor (6192c6b x64 run 39560313916, 6aa85f8 arm64 run
  39560662137). The `coverage` step passes `--exclude-files 'third_party/*'`, which drops
  egui-winit's 30 lines too: 78.43 % (8775 of 11188 lines) in `:latest` x64, every crate floor kept.
- **The Windows x64 build works with clang-cl as `CC`, and the lane builds that way** (2026-10-21).
  tract-linalg 0.23.8 (0.23.9 alike) renders its x86_64 kernels as MASM on the MSVC target and
  sends the AVX-512 VNNI pair through cc-rs, so clang-cl, whose GAS probe passes, got MASM text:
  `invalid instruction mnemonic '_text'`, then `failed to run custom build command for
  tract-linalg`. OmniAccelerANT's clang-cl build hit it; this repo's x64 lane stayed green because
  cc-rs fell back to MSVC's `cl` there, against the family rule. `third_party/tract-linalg-0.23.8`
  is the published crate (sha256 `1cc37ab3…a222`, the index's and the old lock's) with one
  `build.rs` line changed, `if !use_masm() && assembler_supports_avx512vnni() {`, routed in through
  `[patch.crates-io]`; its `PATCHED.md` holds an upstream-ready report. The x64 lane
  (`Invoke-WindowsLane.ps1`) sets `CC` and `CXX` to the image's clang-cl. The arm64 cross lane and
  the local Stevedore driver still take cc-rs's default (`BACKLOG.md`).
  - **DirectML says when it is not there.** The detector registered DirectML through
    `with_execution_providers`, which drops a failed registration silently, and then logged
    `ORT DirectML execution provider enabled` regardless. It registers directly now and warns
    `ORT DirectML provider unavailable, running on the CPU: <ORT's reason>` on failure. The
    session's fate is unchanged: the arm64 chain ORT does carry the DirectML EP (`providers:
    ['DmlExecutionProvider', 'CPUExecutionProvider']` on `windows-11-arm`, hub run 39544745432),
    and where no D3D12 adapter matches, ORT refuses the registration (`No devices detected that
    match the filter criteria`) and the session runs on the CPU.
  - Proven in `:winamd64` (2026-10-21): the x64 lane (`Invoke-WindowsLane.ps1 -TargetArch amd64`)
    passed in 32 min with every cc-rs build on clang-cl (`CC = Some(clang-cl)` in the build
    scripts' output), 381 workspace tests in 32 binaries, every `Build-Windows.ps1` step through MSI,
    and the bundle's `onnx-runtime` and `media-check` on the host. With the lane's `CC`,
    `cargo build -p tract-linalg` exits 101 (`'_text'`) from crates.io and 0 vendored. In Server Core
    the detector logged the DirectML warning above and still built its ORT session; `real_model.rs`
    found both cats on ORT there. In `:latest` the Linux steps `ort-chain-only`, `security`,
    `fmt-clippy`, `test` (720 passed, the one known ignore) and `release` passed with the vendored
    crate.
- **rustls-webpki 0.103.16 and webpki-roots 1.0.10** (2026-10-20): RUSTSEC-2026-0382 and -0381, published
  that day, failed both Linux lanes' security step; ObservANT's ureq takes both. webpki-roots 0.26.11,
  lockfile-only behind torch-sys's ureq 2 and with no fixed 0.26 release, joins `.cargo/audit.toml`.
  `deny.toml` drops the paste and ttf-parser ignores, which cargo-deny now calls stale.
- **The WebGPU demo draws in the browser again** (2026-10-20). The docs site's demo stayed white:
  Chrome refused `forward_pipeline_layout` (`bindGroupLayoutCount (5) is larger than the maximum
  allowed (4)`), so all four forward pipelines and every forward pass were invalid, and Tint refused
  `tonemap.wgsl` (`'@location(0)' appears multiple times`). The tile light grid and its indices moved
  from group 4 into group 3 beside the punctual lights (bindings 1 and 2, one layout and bind group in
  `render/forward.rs`), and `tonemap.wgsl`'s vertex stage returns `FullscreenVsOut` itself, both
  regenerated from BeschleunigerBallett's Slang. Native tests never saw it: `GpuContext` asked for
  the adapter's `max_bind_groups`; it asks for `wgpu::Limits::default()`, WebGPU's, everywhere now,
  and `tests/shader_export.rs` fails on a `@group` past 3 and on a repeated `@location`. On the old
  tree those two tests and 57 GPU tests failed under `KATAGLYPHIS_REQUIRE_GPU=1` (lavapipe, `:latest`);
  the new one passes them all, and Chrome 155 on SwiftShader draws the demo with no WebGPU warning.
- **A `.gltf` with `data:` images loads from memory** (2026-10-20). gltf 1.4's `import_slice` refuses
  every image URI when it has no base path, an embedded `data:` one too, so the web demo's "PBR
  textures" scene failed with `external reference in slice only import`. `load_gltf_slice` imports
  the images itself after refusing an external one by name; its two new tests in `tests/headless.rs`
  failed on the old loader with that error, and Chrome 155 now loads all five demo scenes.
- **The Linux lanes' clippy passes again** (2026-10-19). `EngineConfig::on_detections`, added with
  the ESP32 bridge in 0a4f9a7, failed `clippy -D warnings` with `type_complexity` at
  `src/webcam_engine.rs:34`; its type is the `DetectionsHook` alias now. `ci-container-steps.sh
  fmt-clippy` in `:latest` reproduced the error before the change and passes after it.
- **A board found only through `/dev/serial/by-id` reports its MAC** (2026-10-19). Its serial
  number kept udev's interface suffix (`E8:06:90:9D:3E:78-if00`), so `DeviceInfo::mac()` was
  `None` for exactly the board the libudev-less enumeration missed. `crates/esp32/src/discovery.rs`
  cuts the suffix; the new by-id test failed on the old code with that string.
- **The ESP32 bridge is tested, and the Linux lanes' coverage passes again** (2026-10-19). 0a4f9a7
  added 747 measured lines, 99 of them run by a test, so run 39220063931 stopped at `70.00% coverage,
  7468/10669 lines covered` against `--fail-under 70`. The tests check behaviour: the protocol's twelve
  wire tags, a byte-exact Ping frame, the decoder's foreign version, oversized length, unknown variant
  and full buffer, and three proptest properties (`crates/esp32-protocol/tests/framing.rs`); the link
  over an in-memory serial line through the new `Link::start` (handshake retry, events in order, echoes
  ignored, sequence numbers, the detection throttle and per-mille clamp, write and read failures, the
  6 s idle disconnect, close and drop); discovery's port mapping and order; the frb API's event mapping
  and every command with and without a link; the webcam engine's detections hook. The API's connect
  checks and fills the link slot under one lock, so two connects cannot both pass. `ci-container-steps.sh
  coverage` in `:latest`: 72.89 % (7782/10676); `crates/esp32` (92.9 %) and `crates/esp32-protocol`
  (98.8 %) get floors of 91 and 97, `src/` (14.6 %) 13 instead of 5.
- **The rustdoc logo shows on every page of rust.jonasheinle.de** (2026-10-19). `html_logo_url` was
  `../logo.png`, which reaches the site root only from `oxidant/*.html`; one level deeper
  (`oxidant/api/…`, 60 of the 64 logo pages in today's build) it named `/oxidant/logo.png`, a 404.
  It is `https://rust.jonasheinle.de/logo.png` now, the file `cargo_build_doc.sh` copies there from
  `images/logo.png`.

## [2.3.4] - 2026-10-16

### Added

- **Releases** (2026-10-16, owner). Every push to `main` runs the new `release.yml`: a version
  check against `Cargo.toml` and the CHANGELOG, all five lanes, then the tag `v<version>` and a
  GitHub release with the Linux packages for x64 and arm64, the Windows MSI, MSIX and portable
  zip for both arches and a `SHA256SUMS`, only when every lane is green.
  `scripts/linux/collect-release-assets.sh` picks the files (each flatpak gets its arch, the
  portable folder is zipped, test bundles and reports stay out; 9 checks in
  `test-collect-release-assets.sh`, also run on a real release run's uploads), the hub's
  `publish-github-release.sh` publishes them. `linux-x64.yml` no longer runs on pushes to `main`
  itself, since `release.yml` calls it there.

- **ObservANT's `/visits`: every visit of the last 30 days** (2026-10-15). `/healthz` lists only
  the last 20 visits, too few for statistics per day or hour. `Status` now keeps a history of
  `[arrival, stay]` pairs in `visits.json` beside `recent`, at most 30 days and 10 000 visits; a
  file from before starts it from its `recent`, and a restart closes a visit it cut short at
  `last_seen`, as it does for `recent`. `GET /visits` serves it under the same host check and peer
  CORS as `/healthz`. Tests: counting, stays and a restart, the 30-day edge and the cap, an older
  file, and the endpoint.

- **Mutation testing of the capture loop, and the riscv64 lane meets the real model**
  (2026-10-15). cargo-mutants on `capture.rs`: 87 of 124 compiling mutants caught at first, 98
  now. The arrival photo's privacy test only checked that the filtered photo differs, so a
  filter swapped between on and off passed; it now decodes the JPEG and wants the corner far
  from the cat flat with the filter and sharp without. New tests for `bus_says_done` (an error
  with its text, end of stream as done) and for the restart pause, now `sleep_unless`, which
  fails after 5 s instead of hanging when it never ends; a zero width is not divided by. The
  riscv64 test step fetches the model and sets `KATAGLYPHIS_REQUIRE_MODEL=1` like the Linux one:
  its real-model tests reported `ok` having skipped. The Windows lanes do the same and are in
  `BACKLOG.md`, since their tests run before the model is fetched.

- **Mutation testing of ObservANT's frame, clip and stats code** (2026-10-15). cargo-mutants on
  `clip.rs`, `frame.rs`, `stats.rs`, `output.rs` and `precompress.rs`: 376 of 412 compiling
  mutants caught at first, 400 now. The privacy filter had the most gaps, because its tests used
  one-colour chroma: they now check the 15 % margin to the pixel in both directions, that a block
  takes its own mean, and that the colour planes are kept inside a box and pixelated outside.
  New tests also pin the box outline's exact pixels at every thickness and at the frame's last row
  and column, a box with swapped corners, each pixel's own luma and 2x2 chroma in `to_rgba`, the
  clip's frame counts (`frames_kept`, split out of `Clip::spawn`), a new scratch directory per
  encode, and the stats sampler reading `/proc`. The output pipeline's tests fail after 10 s instead
  of hanging when `run` never returns. Of the 12 left, 9 only widen the privacy filter's
  whole-block shortcut, whose per-pixel path paints the same picture.

- **Mutation testing of ObservANT's camera choice, config and webhook** (2026-10-15). cargo-mutants
  on `camera.rs`, `config.rs` and `notify.rs` caught about 90 % of 145 compiling mutants; now 134.
  New tests: a webcam counts by its UVC driver or its USB bus, and needs a format it can decode;
  `rpicam-vid` without a listed sensor leaves the choice to libcamera; a configured `/dev/video`
  path the providers list is that device; every config limit is valid itself (0 inference fps,
  60 s clips, a one-port ICE range, http and https webhooks); the retries fit inside the 10-minute
  age limit; a short webhook host keeps its path out of the log. The 11 left read real camera
  devices or change a log line. Two decisions in `probe` became `probe_rpicam` and
  `asks_providers`, so they are tested without a Pi camera, and `status.rs` uses
  `notify::unix_now` instead of a copy of its own; the behaviour is unchanged. The webhook tests
  fail after 10 s instead of hanging when no event comes.

- **The camera's restart backoff is tested** (2026-10-15). `Capture::run` waits 1 s doubling to 32 s
  between failed captures, starts over after one that ran more than a minute, and re-probes at once
  when a stand-in test pattern ran its course; the audit found none of it tested. The rule is the
  `Backoff` type now, unchanged, with tests for each case and both boundaries.

- **Mutation testing of ObservANT's arrival logic** (2026-10-15). cargo-mutants planted 393 bugs in
  `motion.rs`, `presence.rs` and `status.rs`; the tests caught about 92 % of the 379 that compile.
  New tests for each gap: a box reaches exactly one block around itself and needs two moving, and
  does not wrap past the right edge; a change of 12 is noise and 24 of 4 800 blocks are motion;
  motion within a period waits for the next look; an IoU of exactly 0.5 keeps a detection; only a
  missing visits file falls back to the old name; a name and the ignored spots may reach their
  limits; a frame 21 s old is a problem and 20 s not; the counters and the failure count; visit
  times are wall-clock seconds. Now 375 of 379; the 4 left change a log line or a bound no client
  sees.

- **Telemetry tests that test it** (2026-10-15). `kataglyphis_telemetry` stood at 8.4 % with
  tests that built a struct and read it back, and one that expected the process-wide counters
  at zero, which a second recording test would have broken. Now: `tick` turns frames,
  inferences and their durations into rates over the elapsed time and falls to zero when idle;
  the GPU fields appear only when sampled; `write_line` appends; the monitor thread writes every
  field to its log file and stops on drop.

- **Mutation testing of ObservANT's network-facing code** (2026-10-15). cargo-mutants planted 163
  bugs in `web.rs` and `static_files.rs`; the tests caught about 84 % of those that compile. New
  tests for each gap: every content type, the gzip cache (once per version), route resolution
  (a file, a directory's index, an app route, a missing asset), the peer origin echo, bytes the
  signalling server sends with its 101, a response head split over packets, an endless one cut
  off at 16 KiB before the handshake timeout, and a refusal re-framed without the upstream's
  `Transfer-Encoding`. Now 142 of 145; the 3 left are equivalent (AGENTS.md).

- **Tests from the audit of 2026-10-15** (the owner asked whether the tests catch regressions):
  - *The bridged core is tested with its real features.* `TEST_FEATURES` adds
    `oxidant/gstreamer,oxidant/onnxruntime`, so `src/webcam_engine.rs` and the webcam API
    compile for tests at all; `src/` stood at 6.2 %. Eight tests: the engine on
    `videotestsrc` with the real model, headless, delivers frames in order with an inference
    time and a frame rate and stops its worker (the channel closes); a missing model, a
    missing texture library and a library without `knt_push_frame` each say so; the rolling
    frame rate; the probe and the default plugin name.
  - *Per-crate coverage floors.* `scripts/linux/check-crate-coverage.py` grades each crate
    and `src/` from tarpaulin's `cobertura.xml` against its own floor, so the renderer's
    90 % no longer carries a crate at 8 %. Its suite plants a crate under its floor. The
    floors sit just under that day's figures (observant 82.7 %, renderer 90.8 %, inference
    79.2 %, cli 74.1 %, core 63.6 %, telemetry 8.4 %, `src/` 5.7 %), and tarpaulin gets
    `--timeout 300`. Measuring them showed that tarpaulin 0.37.5 leaves every
    `#[cfg(feature = …)]` module out of its report (`crates/media`, `src/webcam_engine.rs`),
    so those modules' new tests count as tests, not as coverage (AGENTS.md).
  - *Property tests on what ObservANT reads from the network and its config file*, in each
    module's `mod properties`: the Range header (a served range never leaves the file, a
    well-formed one is served as asked), the query, the Host/Origin check (a public name is
    refused unless allowed, an address literal passes), request paths (none, however
    encoded, leaves the web root), the camera spec and the config (a config that validates
    keeps its numbers in range). Two planted bugs, `..` let through and an unclamped range
    end, each fail them with a shrunk input (`..`, `bytes=0-` on one byte). They replace
    `tests/fuzz_test.rs`, which exercised a placeholder function of its own.
  - *`KATAGLYPHIS_REQUIRE_GST=1`* in the test and coverage steps turns the clip and
    `webrtcsink` tests' self-skip into a failure; `cargo test` hides a passing test's skip line.
  - *Benchmarks against the median of the last 20 runs.* `bench-report.py` warns above
    +25 % and fails a mean from 100 µs that doubles, once 3 earlier runs exist; the cache
    keeps a small `.bench-history/` instead of `target/criterion`. `lint-gates.yml` runs the
    script suites (`scripts/linux/tests/`).

- **Criterion benchmarks, tracked in CI** (2026-10-14, owner: benchmarks belong in each project's own
  tooling, for regression testing). `crates/observant/benches/frame_stages.rs` and
  `crates/inference/benches/detector.rs` run under the existing `cargo bench` step, which now fetches the
  model and runs every bench of the workspace. The Linux lanes cache `target/criterion` from the default
  branch per arch, and `scripts/linux/bench-report.py` writes each mean and its change into the run summary,
  warning above +25 %. First numbers on a Pi 5: a detection 261 ms, I420 to RGBA 1.61 ms, the privacy
  blocks 0.59 ms, a snapshot JPEG 11.0 ms, the motion thumbnail 0.52 ms. They replace the two `#[ignore]`d
  `stage_costs` tests of the same day. `kataglyphis_observant` is a library plus `main.rs`, so benches and
  integration tests can reach its modules.

- **The detector is tested with its real model** (2026-10-14). `crates/inference/tests/real_model.rs`
  runs DEIM-D-FINE-N on a 480 px photo of two cats (`tests/fixtures/two_cats.jpg`, 85 kB):
  both found, the front one at 0.8 or more, every box inside the photo, neither taken for a
  dog or a person, and a class filter keeps only its class. The Linux lanes fetch the model
  and set `KATAGLYPHIS_REQUIRE_MODEL=1`, so there it cannot skip. The inference crate was at
  37.6 % line coverage with no test that ran a model.
- **A coverage floor**: the `coverage` step fails below 70 % of the workspace's lines
  (tarpaulin `--fail-under`); it measured 71.3 % before these tests. Raise it as tests land.

- **`/healthz` says when ObservANT's picture is too dark** (2026-10-14). The motion gate
  already let the model idle on a black picture; `dark` now says so, and `ok` stays, since
  a night without IR is no outage.
- **Two measurements, ignored by default** (replaced the same day by the Criterion benches above). On a
  Pi 5: DEIM-D-FINE-N 420 / 258 / 265 ms at 1 / 2 / 4 threads, preprocessing 3 ms, postprocessing nil, a
  snapshot JPEG 10.7 ms, I420 to RGBA 1.7 ms.

- **Configurable classes, cats by default** (2026-10-14). `classes` (COCO names, `["cat"]`
  unless set) replaces the hard-wired cat filter, in the settings and through
  `POST /settings {"classes": [...]}`, checked whole (one to 80 known names, each once) and
  kept across restarts in `settings.json`; the model's runs read them each time.
  `/healthz` lists `classes`, each visit names its `class` and each `arrived` event too, the
  class of the best detection. `coco.rs` holds the 80 names. `all_classes` still counts every
  class.

- **`/healthz` reports what the cat cam achieves and costs** (2026-10-14): `stats`, sampled
  every 2 s by a thread of its own (`stats.rs`), names the frames pushed per second, the
  model's time per run (smoothed) and runs per second, and from `/proc` and `/sys` its CPU
  share of one core, the cores, its resident memory, the 1-minute load, the board's
  temperature, and its uptime. A number it cannot read is `null`, so off Linux only the
  rates show. The web app's Stats panel reads it.

- **A privacy filter for showing the cat cam in public** (2026-10-14). `privacy =
  "pixelate"` in the settings blocks out everything but the detection boxes (each grown by
  15 %, 32-pixel blocks at 640 wide) in the stream, `/snapshot.jpg`, the arrival photos and
  the clips; the model still sees the whole frame. Only the file sets it; `/healthz` names it
  (`privacy`). `frame::pixelate_outside`, with tests for blocks, kept boxes, edges, the
  capture pipeline and the arrival photo.

- **A visit keeps the boxes it arrived with** (2026-10-14): `/healthz`'s `cat.recent[]` and
  `cat.json` carry `boxes` (fractions), so a page can mark a past false arrival "not a cat";
  older visits have none.

- **`scripts/linux/clippy-windows.sh`** (2026-10-14) runs the Windows lanes' lint, clippy
  `--all-targets -D warnings` for `x86_64-pc-windows-gnu`, on Linux in minutes: as root in
  `:latest` it installs mingw for ring's C and adds the target. With the `I420Layout` slip
  below put back, it fails with the CI's message.

- **Peers, a viewer cap and the build in `/healthz`** (2026-10-14).
  - `peers` (origins) in the config: a GET of `/healthz`, `/snapshot.jpg`, `/arrival.jpg`
    or `/arrival.mp4` whose `Origin` is one of them gets `Access-Control-Allow-Origin` and
    `Cross-Origin-Resource-Policy: cross-origin`, so another cat cam's page can show this
    one; `/healthz` lists them for the page's overview.
  - `max_viewers` (8, 0 for no limit) caps the open `/webrtc-ws` sockets, `503` beyond;
    `/healthz` counts them as `viewers`. Each viewer costs an encoder.
  - `version` in `/healthz` and the start log: `CATCAM_VERSION` at build time, which
    `package-catcam.sh` sets to the package version and commit, else the crate's `-dev`.

- **A cat must move to arrive, and spots can be marked "not a cat"** (2026-10-14).
  - The motion pacer stamps each 8x8 block when it changes; a detection starts a visit
    only with two such blocks from the last 10 s in or around its box (`motion::Moving`).
    An open visit takes every detection, so a sleeping cat stays counted. The first look
    counts as motion everywhere: whatever the camera starts on has just come into view.
  - `POST /settings {"ignore": [[x1, y1, x2, y2], ...]}` (fractions, at most 16) drops a
    detection whose IoU with a spot exceeds 0.5 before it counts, is drawn or scores.
    Kept in `settings.json`; a rotation change clears them. `/healthz` lists the boxes
    drawn now as `cat.boxes`, which the web app's "Not a cat" button posts.
  - `last_score` and the pacer's still-cat rate follow the counted detections, so a still
    false cat costs the idle rate, not half the full one.

- **The cat cam's review fixes (2026-10-13).**
  - Each of the last five arrivals keeps its own picture, `/arrival.jpg?t=<arrival time>`
    as for the clips, so a later false visit no longer replaces the picture a phone
    notification shows. An earlier version's single `arrival.jpg` is taken over.
  - A second arrival while one's clip records gets a clip of its own; overlapping clips
    share their frames, at most three record at once, two wait for the encoder.
  - The webhook tries an event again after 5 s, 15 s, 45 s and 2 min, in order, and drops
    one older than ten minutes: a Home Assistant restart no longer loses a visit.
  - `/healthz` says `ok: false` with a `problem` when no frame came for 20 s or a test
    pattern stands in for a missing camera, and reports `frame_age_s` and `model_age_s`.
  - `/webrtc-ws` and `POST /settings` take requests only from the cat cam's own page: a
    browser's `Origin` must match its `Host`, and the host must be an IP, a bare or local
    name, or listed in the new `allowed_hosts` (another site's page and DNS rebinding get 403).
  - `/snapshot.jpg` encodes once per frame, one at a time, however often it is asked for.
- **Each cat arrival is a video too (2026-10-12).** `cat_webrtc` keeps the last 3 s of
  annotated frames at 10 fps (`clip.rs`, buffers held by reference, no copy), and on an
  arrival adds the next 7 s and encodes them on a thread of its own with `openh264enc`,
  `h264parse` and `mp4mux` (faststart, so playback starts before the end loads).
  `/arrival.mp4` serves it with byte ranges, which Safari's `<video>` needs; `/healthz`
  names the newest one's arrival as `cat.clip`; and `notify_url` gets a `clip` event once
  it is written, with the arrival's time and score, so a notification can add it. The last
  five clips stay in the state directory as `clip-<time>.mp4`, and `?t=<time>` serves that
  arrival's own or `404`, so a false visit right after the cat cannot put its clip behind
  the cat's notification; arrival events carry the visit log's own time for that. `clip_seconds` (default 10, 0 for none) sets the
  length. A GStreamer without one of those elements records no clip and says which.
- **The page changes the cat cam's settings at run time (2026-10-13).** `GET /settings`
  answers the `name`, `rotate` and `score`; `POST /settings` takes any of them as JSON (JSON
  only, so another site's form cannot send it) and checks the whole post before it applies
  any of it. A turn (0, 90, 180 or 270 clockwise) reaches `videoflip`, always in the capture
  pipeline now, on the next frame, with no restart; a score (0.05 to 0.95) the next model
  run; a name the next webhook event. `/healthz` reports `rotate`, `score` and the `frame`
  size and rate. What the page set is kept in the state directory's `settings.json` and wins
  over the config file, key by key. The frame caps carry square pixels, so a quarter turn is
  bordered into the frame instead of stretched: before, 90 and 270 squeezed the turned 3:4
  picture into 4:3. `rpicam-vid --rotation 180` is gone; 180 goes through `videoflip` like
  the other angles. OmniAccelerANT's Camera page is the front end.
- **`--precompress DIR` ships a web build smaller (2026-10-12).** It writes a zopfli
  `<file>.gz` beside every file the server would gzip, on every core, and the server sends
  that file instead of compressing its own, unless the original is newer. OmniAccelerANT's
  packager runs it: on a Pi 5, a phone's first visit to the cat cam page went from about
  4.62 MB to 4.46 MB (`skwasm_heavy.wasm` 2291 to 2214 kB, `main.dart.wasm` 1479 to
  1418 kB; plain HTTP at a LAN address is no secure context, so Flutter loads the heavy
  renderer), 3.93 to 3.75 MB over loopback, and nothing is compressed or held in memory
  for it at run time. `zopfli` 0.8.3; its dependencies
  were in the lockfile already.
- **The kept clips are listed, and a departure names its arrival (2026-10-12).**
  `/healthz` gains `cat.clips`: the arrival of every clip `?t=` still serves, newest
  first, for a page that lists them all. A `left` event carries the arrival it ends
  (`arrived`) and the score that arrival was confirmed at (`best_score`), so a receiver
  can update the notification it sent for that arrival and drop a false visit's
  departure under the same threshold. A `clip` event of a visit already over carries its
  `stayed_s`, as a visit shorter than the clip's post-roll ends before the clip is written.

### Changed

- **The version lives in `Cargo.toml` alone: 2.3.4** (2026-10-15, owner). `[workspace.package]`
  said 0.1.0 while `VERSION.txt` said 2.3.4, and CI, the packages and the MSIX read the text file.
  `VERSION.txt` is gone; `compute-version.sh` and both Windows lanes' `version-file` hand the hub's
  `version_util.sh` the `Cargo.toml`, and a local Windows build's MSIX version comes from it through
  `Get-CargoMsixVersion` (`WindowsCargoTarget.Common`) instead of `Build-Windows.config.psd1`'s
  `0.1.0.0`. Every workspace crate is 2.3.4, so ObservANT's `-dev` build reports `2.3.4-dev`.

- **CI: a push to `develop` runs Linux x64 and the gates; the other build lanes run on request**
  (2026-10-15, owner rule for the family, ANTfrastructure's `docs/branch-and-release-strategy.md`).
  Linux arm64, Linux riscv64, Windows x64 and Windows arm64 lost their push and pull-request
  triggers. A commit message asks with `[ci <lane>]` (`[ci windows]`, `[ci arm64]`,
  `[ci riscv64]`, `[ci all]`), which the new `on-request.yml` reads through the hub's
  `requested-lanes` action, and each lane has *Run workflow*. Each lane's concurrency group is
  its literal name now, since `github.workflow` is the caller's inside a called workflow and
  would put every requested lane in one group. riscv64 keeps its weekly GPU run.
- **The default model is DEIM-D-FINE-N (Apache-2.0), fetched rather than tracked**
  (2026-10-14, owner). The AGPL-3.0 `resources/models/yolov10m.onnx` (59 MiB) is no longer
  tracked; `MODEL_FILE` is `deim_dfine_n_coco.onnx` (15 MiB, COCO 80, 43.0 AP), built from
  DEIM's pinned checkpoint by [ModelANT](https://github.com/Kataglyphis/ModelANT) in the
  same `[1, 300, 6]` layout, so the parser and the default plain-resize preprocessing stay.
  `resources/models/models.lock` names it, its license and notice with URL and SHA-256;
  `scripts/linux/fetch-models.sh` and `scripts/windows/Get-Models.ps1` (the Windows build's
  *Fetch Models* step) fetch and check them, through a cache. Against YOLO26n on 12 arrival
  pictures of three cat cams it found cats more often and more surely, at about 260 ms a
  run instead of 190 ms on a Pi 5. The burn demo `onnx-yolov10` is `onnx-detector` (the old
  name stays an alias). Test fixtures use generic private addresses, not the cat cams' real ones.
- **The cat cam is ObservANT** (2026-10-14, the owner's name for it). Cats stay the default,
  detecting is general. The crate is `crates/observant` (`kataglyphis_observant`), the Pi
  script `scripts/linux/observant/`. `/healthz`'s `cat` object is `detection`, the visits file
  `visits.json`, the settings `observant.toml`, the variable `KATAGLYPHIS_OBSERVANT_CONFIG`, the
  build stamp `OBSERVANT_VERSION` and the default name `ObservANT`. An install from before
  carries over: `catcam.toml`, `cat.json`, `KATAGLYPHIS_CATCAM_CONFIG`, `CATCAM_VERSION` and
  Windows' `omni-accelerant-catcam` folder are still read while the new ones are absent.
- **A cat back within a minute continues its visit** (2026-10-14). At the feeding trough
  one cat walking about had made 21 visits in an hour, 10 to 90 s apart, each with its own
  arrival event, clip and count. Being in view still ends after about 3 s without a hit (the
  boxes, the motion gate and the still rate follow it); the visit ends only a minute after the
  last sighting, so the departure event comes a minute later and `stayed_s` spans the whole
  stay.
- **A Pi camera turned a quarter captures the picture as small as it ends up** (2026-10-14).
  At 90 and 270 degrees `rpicam-vid` and libcamera deliver 480x360 (for 640x480) through
  the ISP's free scaler, so `videoscale` only adds the side borders. Measured on a Pi 5,
  30 fps: resampling the turned 480x640 cost 20 % of a core, borders alone 3 %. A turn
  between landscape and portrait restarts such a capture once (`Ended::Turned`); other
  sources keep their size and still turn live. On tabblscatcam (rotate 270) the service
  went from 21 % to 4 % of a core.
  - `rpicam-vid --codec yuv420` pads each row to 128 bytes: 480 wide writes 512-byte rows
    (276480 bytes a frame, measured). `rawvideoparse` now gets those strides and offsets;
    read as tight rows the picture was green stripes. Only a width that pads alike at 64
    and 128 bytes is captured smaller, so the stride holds on either ISP.
- **Renovate skips the vendored `third_party/egui-winit-0.36.2` (2026-10-14).** A packageRule
  in `.github/renovate.json` (`matchFileNames`, `enabled: false`) keeps its upstream
  `Cargo.toml` and `Cargo.lock` out of the report and out of lock maintenance. Its rows had
  made the 2026-10-07 apply refuse its whole run, and cargo exits 101 on that lock. The
  egui family stays watched through the root `Cargo.toml`. The hub's planner
  (`renovate_planner.py lockmaint`) now lists only the root and crate manifests.
- **The six Linux feature checks run in one job (2026-10-13).** `linux-x64.yml`'s
  `feature-matrix` pulled the 12.8 GB image six times, once per feature set, and each job
  spent 6-9 min on that prologue for 1-3 min of `cargo check`: 56-67 runner-minutes a run
  for about 13 min of work (runs 38394017265, 38400294322, 38407158265). Now one job runs
  `bash scripts/linux/ci-container-steps.sh feature-check`, which checks every set in
  `FEATURE_CHECK_SETS` in one container and one warm target directory. Each set has its own
  log group and, on failure, its own `::error` annotation, and every set runs even after
  one fails. All six pass that way in `:latest` on the dev box. Dispatching `linux-x64.yml` with
  `feature-checks-per-job` brings back one job per set.

### Fixed

- **`inference = "auto"` runs the model on 1 GB boards** (2026-10-15). It skipped the model
  below 1024 MiB of RAM to spare a 512 MB Pi Zero 2 W, but the GPU keeps part of every board's
  RAM: a 1 GB Pi 3 Model B reports 905 MiB, so it streamed without detection. The threshold is
  768 MiB (`auto_inference_fits`), between the two, and a test pins both boards' figures.

- **An infinite box coordinate no longer overflows `Moving::near`** (2026-10-15). `index(v) + 1`
  overflowed for `inf` (`usize::MAX`), a panic in a debug build; it saturates now, and a box far
  past the frame is still cut to it. Found while checking the last mutant of the arrival logic.

- **`KATAGLYPHIS_SCORE_THRESHOLD` and `KATAGLYPHIS_INFER_EVERY_MS` take surrounding spaces**
  (2026-10-15), as `KATAGLYPHIS_ORT_THREADS` already did: `" 0.4"` fell back to 0.5 without a
  word. `crates/core`'s env readers parse through pure functions now (`parse_preprocess`,
  `parse_or`, `parse_log_level`), so the rules have tests; the crate had none of its own.

- **A missing model file says so** (2026-10-15). `PersonDetector::new` answered a wrong path
  with "No ONNX backend available. Build with --features onnx_tract…", which sends one
  looking for a missing runtime. It now checks the file first ("no ONNX model at <path>"),
  and when ORT fails on a file that exists, its own reason is kept ("ORT could not load
  <path>: …") instead of the generic line. Found by the new webcam engine test;
  `crates/inference/tests/model_errors.rs` pins both messages.

- **`list_cameras` no longer runs on the caller's thread** (2026-10-15). It was
  `#[frb(sync)]`, and the device monitor it starts brings libcamera up: OmniAccelerANT's
  Stream page called it as its feature probe, which held the app's first frame 0.94 s on a
  Pi 5. It is async now, and `webcam_inference_available()` answers the probe from
  `cfg!(all(feature = "gstreamer", onnx))`, which also stops a gstreamer-only build from
  offering a pipeline whose start would fail.
- **ObservANT, from the deep review of 2026-10-14:**
  - *Reads need a local host name too.* `own_host` now runs on every request, so a page
    that DNS rebinding pointed at the board cannot read `/snapshot.jpg`, the arrival
    photos or the clips as its own; `own_page` adds the `Origin` check for the acting ones.
  - *No connection flood.* At most `MAX_CONNECTIONS` (128) are served at once, a newer one
    is closed unanswered and logged once a minute, and hyper gets a timer, so a request
    head must arrive within 10 s (without one hyper 1.x skips its header timeout).
  - *A failing model shows.* Three failed runs in a row make `/healthz` say `ok: false`
    with `the model fails: …`; every failed run clears the drawn boxes, which stayed sharp
    under the privacy filter before; the log keeps the first failure and one line a minute.
  - *The privacy filter locks the classes.* With `privacy = "pixelate"`, `POST /settings`
    refuses other `classes` and classes a page saved earlier give way at start, since a box
    of `person` stays sharp; `GET /settings` names `privacy`.

- **A browser that vanished frees its viewer place** (2026-10-14). The web server's
  connections carry TCP keepalive (60 s idle, then a probe every 10 s), so one whose peer
  slept or left the Wi-Fi without closing ends in about 2.5 minutes, with its `/webrtc-ws`
  socket and its `max_viewers` place. Before, nothing ended it until the server sent
  something, and enough of them would have kept new tabs out.

- **Windows arm64 built again after the quarter-turn capture** (2026-10-14). `I420Layout`
  serves only the `cfg(unix)` rpicam source, so Windows' clippy `-D warnings` called it dead
  code (run 38496162066); it is `cfg(unix)` now, and the next Windows round passed.
- **A GStreamer without the clip encoder no longer leaves a scratch directory per arrival
  (2026-10-13).** `encode_mp4` made its directory before it built `openh264enc`,
  `h264parse` and `mp4mux`, and returned early when one was missing, past the cleanup at
  its end. Windows arm64's GStreamer has no `openh264enc`, so the test that a failed
  encode leaves nothing behind failed there from a9e3b58 on, while x64 stayed green. The
  directory is removed on drop now, after the pipeline; a test hides `openh264enc` from
  the registry for one encode, and fails against the old code (939990b).
- **The ORT loader accepts the chain ORT of the 2026-10-08 `:winamd64` (2026-10-10).** That image's
  `onnxruntime.dll` names its sources `C:/temp/onnx-src/onnxruntime/core/...`, with forward and mixed
  separators, and no longer the backslash form `verify_chain_build` looked for. So every Windows ORT
  session fell back and failed, and OmniAccelerANT's Windows cat cam test lost its cat. `embeds_marker` now
  treats `/` and `\` alike, as it already ignored case. In `:winamd64`, the cat cam found 2 cats (best score
  0.72) with the image's ORT, where it failed before the change.
- **The cat cam runs ORT on two threads that sleep between runs (2026-10-11).** ORT's
  default, four threads spin-waiting for the next run, kept himbeere2's live service at a
  133 % average over 5.8 days at 2 Hz. On ANThology's photo the process took 183 % that
  way and 81 % now, with the same detections. Installed there, the live service went from
  131 % to 70 % of a core (60 s each, same camera, same scene). `inference_threads` sets
  the count. The inference crate keeps ORT's default for every other caller, and
  `KATAGLYPHIS_ORT_SPIN` / `KATAGLYPHIS_ORT_THREADS` set both for any of them.
- **A cat counts after two hits in a row (2026-10-11).** A lone detection near the score
  threshold logged an arrival and a departure a second apart, and drew a box for half a
  second. 52 of the 72 arrivals in himbeere2's journal were that. A cat now leaves after
  3 s without a hit (longer at slow rates). The journal keeps its words.
- **The cat cam's frames stay I420 from camera to encoder (2026-10-12).** The capture
  asked for RGBA, so every frame was converted from the camera's I420 and back for the
  encoder. Boxes are drawn into the YUV planes now, and only the model's frames and
  snapshots are converted. himbeere2's idle service went from 13 % to 9 % of a core. On
  ANThology's photo the best score moved from 0.72 to 0.76: the photo now passes through
  chroma subsampling, as a camera's frames always did.
- **A Pi camera mounted upside down is flipped by the sensor (2026-10-11).** `rotate = 180`
  becomes `rpicam-vid --rotation 180` instead of a `videoflip` over every RGBA frame, which
  halved the capture thread on himbeere2 (11 % to 5 %). Two 4 s stills of its imx219, with
  and without the flag, show the same scene turned 180°, as the `videoflip` turned it.

### Added
- **The cat cam keeps a log of the cat's visits (2026-10-12).** `/healthz`'s `cat.recent`
  lists the latest 20, newest first: `arrived` in Unix seconds and, once the cat has left,
  `stayed_s`. The log lives in `cat.json` beside the count, and a `cat.json` from before it
  still loads. The service also writes that file once more when it stops, so the next start
  closes a visit the stop cut short at the cat's last sighting; after a power cut, at the
  last arrival or departure written.
- **The cat cam remembers its cat across restarts (2026-10-12).** Visits, the last
  sighting and score, and the arrival frame are written to systemd's `$STATE_DIRECTORY` on
  every arrival and departure and read back at start, so an upgrade or a reboot no longer
  turns the Stream page back to "No cat seen yet".
- **The cat cam tells a webhook when a cat comes or goes (2026-10-12).** `notify_url`
  gets `{"event": "arrived"|"left", ...}` as JSON over http or https, from a thread of its
  own; `/arrival.jpg` is the frame the arrival was confirmed on, boxes drawn. Logs never
  carry the URL's path, which is a Home Assistant webhook's secret.
- **The cat cam serves its web build gzipped (2026-10-12).** The Flutter boot's main files
  went from 20.2 MB to 7.5 MB for a browser that accepts gzip. Each file is compressed on
  its first request (614 ms for the 7 MB CanvasKit on a Pi 5) and kept in memory while it
  is unchanged. A revalidation, the common request of a return visit, no longer reads the
  file at all.
- **The cat cam idles its model while nothing moves (2026-10-11).** `motion.rs` compares
  8x8-block mean luma between looks; with no motion and no cat in sight the model runs at
  `idle_inference_fps` (default 0.2) instead of `inference_fps`. A still scene without a
  cat dropped from 74 % to 12 % of a Pi 5 core at 2 Hz. A cat in view, a fresh hit or
  motion keeps the full rate. `/healthz` gains `model_runs`. A black frame (2026-10-12)
  skips the idle run as well, which took the dark room at night from 9 % to 2 %.
  A cat in view with nothing moving is looked at every `still_period`, twice the full
  rate's period (2026-10-13); motion restores the full rate, and the visit lingers past
  the still period. himbeere2's drying rack, a false cat at 0.34, went from 69 % to 36 %.
- **`/snapshot.jpg` and the cat in `/healthz` (2026-10-11).** The cat cam's web server
  encodes the latest frame, boxes included, when asked. `/healthz` gains `cat`: whether
  one is in view, when one was last seen, the visits counted and the last best score.
- **`crates/cat_webrtc` is the cat cam service (2026-10-05).** One process now does what
  a container, nginx and a board-local script did:
  - It picks the camera: a Pi camera through `rpicam-vid`, else a libcamera CSI camera,
    else a UVC webcam. With none it streams a test pattern and re-probes every 30 s.
  - It serves OmniAccelerANT's web build with `/webrtc-ws` proxied to its signalling
    server, which moved to `127.0.0.1` by default.
  - It reads a TOML config under the flags, pins the ICE ports, turns STUN off by
    default and stops cleanly on SIGTERM.
  - Measured in `:latest` (x64), with nothing else running:
    - Headless Chrome played the stream from the built-in server over plain HTTP from a
      non-loopback address: 640x480, 875 frames in 29 s.
    - Every ICE candidate sat in 40000-40099.
    - Inference found both cats in ANThology's photo (best score 0.59).
    - A missing camera and a broken source recovered as designed.
  - **On Windows the exe runs from its install folder (2026-10-06).**
    - `install.rs` points GStreamer, ONNX Runtime and the model at the folder beside the
      exe, and the settings file moves to `%ProgramData%\omni-accelerant\catcam.toml`
      there.
    - In a `:winamd64` rebuilt with the hub's WebRTC fixes, OmniAccelerANT's MSI
      installed it, and it ran with only System32 on PATH, loading modules only from that
      folder and Windows.
    - A `webrtcsrc` viewer decoded 60 frames, and the bundled `yolo26n` on the bundled
      chain ORT found the cat.
  - **First board run: himbeere2 (Raspberry Pi 5, imx219, Debian 13), 2026-10-05.** It
    ran as OmniAccelerANT's packaged service and picked the camera via `rpicam-vid`.
    Headless Chrome on another host played it at 29 fps. Three fixes came out of it:
    - `rpicam-vid` is now owned by a guard. A failed pipeline setup no longer leaves one
      zombie per retry.
    - Probes skip the device providers when rpicam has the camera, and probe nothing for
      `test` and `image:`. The host's libcamera, with its TensorFlow Lite and abseil, no
      longer loads into the service.
    - `inference_fps` (default 2, `--inference-fps`) caps the model's rate. Without a
      cap, YOLO held the Pi 5 at 380 % CPU; capped, it uses 132 % of one core.
  - **`run-producer-pi.sh` streamed from that board's CSI camera on 2026-10-06, the first
    time outside QEMU.**
    - `--build` built the producer, and the image's entrypoint kept `/hostlibs` ahead of
      its own libcamera (CON23's order), so `libcamerasrc` ran the host's `v0.7.2+rpt`
      with libpisp 1.7.
    - `yolov10m` ran on ORT.
    - Through `serve.sh --http`, Chrome on another host played 701 frames in 23 s.
    - A trap for anyone sharing the `kataglyphis-cat-target` volume: a build from the
      superproject layout leaves `env!("CARGO_MANIFEST_DIR")` pointing at
      `/workspace/third_party/OxidANT`. Cargo sees the same relative paths under the
      runner's mount and does not rebuild, so `--build` keeps that model path. `cargo
      clean -p` on the workspace crates cures it.
- **Every test runs on every Linux and Windows lane (2026-10-01).** Before this, the Linux
  lanes ran 358 tests and both Windows lanes 167.
  - All lanes test with `--features 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'`
    (`TEST_FEATURES` in `scripts/linux/ci-container-steps.sh`). `crates/media`'s
    `capture_test` is `cfg(feature = "gstreamer")` and had compiled empty everywhere. The
    `?/` form selects the member's own feature: the plain one also switched on the optional
    `kataglyphis_media` dependency of `kataglyphis_gui` and the root package
    (`cargo tree -e features`). Linux clippy takes the same features.
  - Windows x64: `Invoke-DebugTests.ps1` runs the workspace minus the renderer, adding
    `kataglyphis_inference` (20), `kataglyphis_telemetry` (4) and the capture tests (2). It
    builds the renderer's lib and all 18 integration test binaries for the host, listed in
    `target\host-tests\tests.json` and checked against `tests\*.rs`. `Invoke-HostTests.ps1`
    runs them through the hub's `Invoke-StagedTests.ps1`, which the arm64 lane uses too.
  - Windows arm64: `Stage-CrossTests.ps1` builds the whole workspace's tests, renderer
    included, with the same features. It stages the product's GStreamer plugins beside the
    capture test, and of the programs only those an integration test starts.
  - Both Windows lanes set `KATAGLYPHIS_REQUIRE_GPU=1`, so a renderer test with no
    adapter fails instead of skipping itself. `GpuContext` carries the `adapter_info`, and
    `headless_or_skip()` writes it once per test binary straight to stderr, past libtest's
    capture, as an `ADAPTER:` line. lavapipe (x64 and arm64) and the x64 runner's WARP
    (`Microsoft Basic Render Driver`, Dx12) all report `bc=true timestamps=true`, so under
    the flag `ktx2::uploads_bc1_when_supported` and
    `gpu_timing::every_pass_reports_a_finite_non_negative_duration` fail rather than skip
    without those features.
  - Both Windows lanes' clippy covers `--workspace --all-targets --locked`, as on Linux; it
    covered the root package, `kataglyphis_cli` and `kataglyphis_gui` only.
  - Coverage passes `--workspace`, the test features and `KATAGLYPHIS_REQUIRE_GPU=1` to the
    hub's `cargo_coverage.sh`. Without `--workspace`, tarpaulin measured the root package
    alone, which has no unit tests: `0.00% coverage, 0/754 lines covered`.
  - Measured at 338c4da, which still carried a separate adapter-report test (360 without
    it): Linux x64 and arm64 361 passed, 1 ignored (runs 36889465993, 36889466121), coverage
    65.23% (4994/7656 lines); Windows x64 361 passed, 1 ignored, 30 in the container and 331
    on the host (run 36889466941). The Windows arm64 run (36889467167) stopped in
    `headless.exe`: the process died after `running 39 tests` with no test finished and no
    panic text, the first rendering any arm64 test had done on WARP (the tests that ran
    before it create devices but draw nothing). Its renderer binaries now run one test at
    a time (`--test-threads=1`), which passes; the parallel crash is not root-caused
    (BACKLOG.md).
  - Measured at 776685b, every lane green and the same set everywhere: Linux x64 and arm64
    360 passed, 1 ignored (runs 36907253295, 36907253309), coverage 65.26% (5000/7662);
    Windows x64 360 passed, 1 ignored, 30 in the container and 330 on the host (run
    36907253841); Windows arm64 `TESTS: passed=360 failed=0 skipped=1` on `windows-11-arm`
    (run 36907253720). WARP reports `bc=true timestamps=true` on both Windows runners.
- **The arm64 cross lane runs the x64 lane's tests on `windows-11-arm` (hub CON43, 2026-10-01).**
  - `Invoke-WindowsLane.ps1 -StageTests` calls the new `Stage-CrossTests.ps1`. It builds
    the four targets of `Invoke-DebugTests.ps1` with `cargo test --no-run` for the target
    triple, from `C:\ws` like the x64 tests.
  - It stages them under their repo-relative paths, together with `kataglyphis_cli.exe`
    (for `CARGO_BIN_EXE`), the per-directory DLL closure, the renderer's source tree and
    fixtures, the hub's `Invoke-StagedTests.ps1` and a `tests.json`.
  - On the runner, the test-command links `C:\ws` to that mirror, so the paths compiled
    into the binaries resolve.
  - The renderer runs with `--nocapture`, and its `SKIP: no GPU adapter` lines count as
    skips, not passes.
  - Measured on x64 in `:winamd64`: 4 test binaries and the CLI staged, and
    `TESTS: passed=4 failed=0 skipped=0`, the renderer left out because Server Core has no
    `opengl32.dll`.
- **The Linux lanes render the WebGPU golden tests, on lavapipe (2026-09-29).** The
  `test` step of `scripts/linux/ci-container-steps.sh` exports
  `KATAGLYPHIS_REQUIRE_GPU=1`, so a missing adapter panics instead of skipping. The
  family image has a software Vulkan device since ANTfrastructure CON19; proven
  locally on the amd64 child (`vulkaninfo`: deviceType CPU, `llvmpipe`), the step
  green with no `SKIP: no GPU` line. The one test that fails on it,
  `a_non_uniform_instance_scale_shades_like_the_same_node_scale` (the known
  instanced-normal bug, 987 px, blocked upstream), is `#[ignore]`d with its reason.
- **Every Windows package carries the GUI's GStreamer plugins and its model, and
  both lanes prove it (2026-09-25).**
  - The camera pipeline creates its elements by name, so their plugins were in no
    import table: the bundle, MSIX and MSI shipped GStreamer's DLLs but not one
    plugin, and the GUI could not start on a machine without the image's
    `C:\runtime`. *Stage GStreamer Plugins* now copies `Build.GStreamerPlugins`
    (`Build-Windows.config.psd1`) into `lib\gstreamer-1.0` beside an exe that links
    GStreamer, and a plugin the image lacks fails the build. The plugins seed the
    DLL closure, so what they import lands beside the exe.
  - The GUI initialises GStreamer through `kataglyphis_media::ensure_gst_initialized`,
    as the webcam engine does, which points `GST_PLUGIN_PATH` at the plugins beside
    the exe; `kataglyphis_media::bundled_plugin_dir` names that directory for both.
  - `kataglyphis_cli media-check` (with `gui_windows`) builds the pipeline without
    starting it and prints the plugin behind each element. It fails on a missing
    element, and when an exe that carries plugins takes one from anywhere else.
    The x64 host step and the arm64 run job both run it on the packaged exe.
  - `resolve_model_path` takes `resources\models\yolov10m.onnx` beside the exe
    before the compile-time checkout path, so a packaged app finds its model with
    no `KATAGLYPHIS_ONNX_MODEL`. The MSI now installs `resources\` too.
  - The MSI installs the bundle's whole tree: its generated group
    (`msi-payload-files.wxs`, was `msi-payload-dlls.wxs`) holds one component per
    file, placed with WiX v4's `Subdirectory`. `New-OrtProvenPayload -IncludeDirectory`
    copies `lib\` into each payload before G6 proves it.
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
  unresolved import. The hub pin moves from 2fdddbc7 to 6b576c27, which
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
- **Lock file maintenance moves 61 packages (2026-10-13, the Renovate report).**
  - `cargo update` takes every in-range release, among them cxx 1.0.205, proptest 1.12.0
    (with rand_xorshift 0.5.0), serde_json 1.0.154, turso 0.8.3, uuid 1.28.0, zopfli 0.8.4
    and syn 3.0.9. `ort` stays `=2.0.0-rc.13`: rc.14 targets ONNX Runtime 1.31.0 with a new
    `Environment` API, and the images still carry 1.30.0. `zune-core` stays 0.5.3.
  - `third_party/egui-winit-0.36.2/Cargo.lock` is again not refreshed. cargo exits 101 there
    (`current package believes it's in a workspace when it's not`), because the crate sits
    inside this workspace without being a member; a copy outside it updates. No build reads
    that lock, and `PATCHED.md` vouches for every file but `src/lib.rs` as published.
  - Verified in `:latest` (x64): `ci-container-steps.sh` `ort-chain-only`, `security`,
    `fmt-clippy`, `test` (457 passed, the one known ignore, lavapipe with
    `KATAGLYPHIS_REQUIRE_GPU=1`), `release` and `docs`; `cargo check` for each of the six
    `feature-matrix` rows; the renderer's wasm32 check; `run-lint-gates.sh --ratchets`.
- **tokio 1.53.1 → 1.53.2, hyper 1.11.1 → 1.12.0, upload-artifact v7.0.1 → v7.0.2
  (2026-10-07, the evening Renovate report).**
  - Both crates are lockfile-only moves (`cargo update -p tokio@1`, then `-p hyper@1`,
    the order `renovate-local.sh --apply` runs them). hyper 1.12 drops `futures-channel`,
    and the lock loses windows-sys 0.60 and the windows-targets 0.53 family with it.
  - The report's five rows for `third_party/egui-winit-0.36.2` (log, serde, webbrowser,
    smithay-clipboard, objc2) were not taken. That `Cargo.lock` is the published crate's
    and is never read: cargo refuses to refresh it because the crate sits inside this
    workspace, which is also why the apply half refused the whole run.
  - Verified in `:latest` (x64): `ci-container-steps.sh fmt-clippy`, `test` (395 passed,
    the one known ignore, lavapipe with `KATAGLYPHIS_REQUIRE_GPU=1`), `security` and
    `ort-chain-only` pass, so do the renderer's wasm32 check and `run-lint-gates.sh`.
- **burn 0.21.0 → 0.22.0 and glam 0.33 → 0.34 (2026-10-07, from the Renovate report).**
  - burn 0.22 drops the backend type parameter: models and tensors are plain types
    (`Tensor<2>`, `nn::Linear`), and the backend is a runtime `Device`. The burn demos
    run on `Device::flex()`, with `.autodiff()` for training. Flex replaces NdArray,
    which 0.22 deprecates.
  - `burn-ndarray` and `burn-autodiff` are gone as direct dependencies. The root package
    enables burn's `flex` and `optim` features instead, and `optim` implies `autodiff`.
  - Two smaller API changes: the optimizer is a `ModuleOptimizer`, and `save-load` writes
    a burnpack file through `Module::save_file`/`try_load_file`, because 0.22 removed the
    `Recorder` API.
  - `tensor-demo` prints `TensorData` through `Display`, because its `Debug` shows raw
    bytes in 0.22.
  - The bincode advisory (RUSTSEC-2025-0141) left both ignore lists: burn-core no longer
    pulls bincode, and cargo-deny reported it `advisory-not-detected`.
  - glam 0.34 removed the deprecated `Mat4` projection and `look_at_*` methods. The
    renderer already used `glam::camera`, so it needed no code change.
  - Verified in `:latest` (x64):
    - `ci-container-steps.sh fmt-clippy`, `test`, `security`, `release` and
      `ort-chain-only` pass, and so does `run-lint-gates.sh`. The test step ran
      395 tests on lavapipe with `KATAGLYPHIS_REQUIRE_GPU=1`, with 0 failed and the one
      known ignore.
    - `cargo clippy --all-targets --features burn_demos -D warnings` is clean.
    - All six `feature-matrix` rows and the renderer's wasm32 check build.
    - Every burn demo ran, including `onnx-yolov10` with adapter training on the chain ORT.
- **arm64's renderer tests run llvmpipe at 128-bit vectors (2026-10-06).** The image's
  256-bit setting (hub CON44, for Mesa's BVH sort) made LLVM's AArch64 JIT kill a renderer
  test binary in four of about twenty arm64 runs since 2026-10-05. Three SIGABRTs were
  `LLVM ERROR: Do not know how to split this operator's operand!` (`forward_ambient`,
  `occlusion`, `headless`) and one a SIGSEGV in `forward_ambient`. OxidANT uses no ray
  tracing and no subgroups, so `ci-container-steps.sh` exports `LP_NATIVE_VECTOR_WIDTH=128`
  on aarch64, the native NEON width.
- **The arm64 parallel renderer crash has a name (2026-10-06).** Dispatch 37422053110 ran the
  renderer tests in parallel on `windows-11-arm`: `ibl.exe` died with `0xC0000005
  STATUS_ACCESS_VIOLATION`, faulting module `d3d10warp.dll` at offset 0x80528 (WARP
  10.0.26100.9278), reported by the hub runner's exit-code and WER naming. The fault is in
  Microsoft's WARP, so the renderer tests stay serial there and the BACKLOG row is closed.
- **The Linux x64 feature check runs on every push (2026-10-06).** `feature-matrix` lost its
  `[build-features]` marker, the last opt-in job here: the owner's rule is that every lane
  runs. Its first run ever, dispatch 37423257667, passed all six rows (`gstreamer`,
  `gui_linux`, `gui_linux,gstreamer`, `gui_windows`, `onnxruntime`, `burn_demos`).
- **The arm64 lane can run the renderer tests in parallel on dispatch (2026-10-06).** The
  `parallel-renderer-tests` input of `windows-arm64-cross.yml` drops Stage-CrossTests.ps1's
  `--test-threads=1` from `tests.json` on the runner, so one run can name the WARP crash
  with the hub runner's exit-code and WER reporting. Push runs stay serial.
- **The Linux lanes ship a deb, an AppImage and a flatpak beside the tar (2026-10-06, hub
  `1b99f7b5`).** `PACKAGE_TYPES` was `tar`, because the hub's `package_archive.sh` wrote the
  tar and stopped. It now builds all four from the release `kataglyphis_cli`, installed as
  `oxidant`:
  - the deb as `/usr/bin/oxidant`, depending on `libc6, libgcc-s1` (the binary links
    `libc`, `libm` and `libgcc_s`);
  - the AppImage;
  - the flatpak as `org.kataglyphis.oxidant` on freedesktop 24.08.

  The lane passes `packaging/linux/oxidant.desktop` (`Name=OxidANT`, `Terminal=true`) and
  `images/logo.png`, a real 128×128 PNG. `packaging/flatpak/` is gone: its `icon.png` was
  the escaped text of a 1×1 PNG, `icon.png.b64` a 1×1 image, and its hand-written
  manifest pointed at `../../target/release/oxidant` and was never read. Proven in
  `:latest` on this binary: the deb's binary runs, the AppImage runs, and the flatpak
  installs and runs through `flatpak run`. The BACKLOG row is closed.
- **The renderer iterates fixed chunks as arrays (2026-10-06).** All 46 sites in
  `crates/webgpu_renderer` that clippy 1.98's `chunks_exact_to_as_chunks` flagged now use
  `as_chunks::<N>()`, so each element is a `&[T; N]`:
  - the glTF and OBJ triangle loops, with `tri.map(|i| i as usize)`;
  - the HDR tests;
  - histogram;
  - IBL, whose BRDF read-back is now a plain `.to_vec()`;
  - LOD and QEM;
  - 26 in the integration tests, which clippy reaches only once the lib compiles.

  The two GPU timestamp read-backs drop their `try_into().expect(…)` for
  `u64::from_le_bytes(*b)`. The workspace-wide allow in `Cargo.toml` is gone. The one
  `chunks_exact` left, `channels * 2` in the 16-bit PNG path, has no constant size.
  In `:latest`: workspace clippy with CI's features and `-D warnings` is clean, rustfmt is
  clean, and the renderer's 163 unit tests and every integration suite pass, the 38
  `headless` GPU tests on lavapipe included.
- **The MSI step and the helper table became hub calls (2026-10-05, hub `b9247b03`, CON55).**
  The *MSI Packaging* step of `scripts/windows/Build-Windows.ps1` calls the hub's
  `Invoke-MsiPackage`. It used to resolve `wix.exe` itself, assemble the `wix build` argv and
  write the payload fragment, about 50 lines. It still decides what the payload is: the G6-proven
  DLLs, `lib\` and `resources\`. `AGENTS.md` § 2 links the hub's
  *Reach for these before writing a helper* table instead of carrying a copy of it. With those
  two, `BACKLOG.md` § *Waiting on ANTfrastructure* is empty and gone.
- **Five local copies became hub calls (2026-10-05, hub `54e7d7fc`, CON55).** The hub's
  `Get-ANTfrastructurePin` replaces `Resolve-CargoToolPin` in `scripts/windows/Build-Windows.ps1`.
  The hub's cargo drivers register `CARGO_SAFE_DIRECTORY`, so `ci-container-steps.sh` only
  exports it; `riscv64-test`, which runs cargo itself, sources the hub's `_cargo_wrapper.sh`.
  The README links the hub's MSIX trust steps instead of repeating them. The hub's unused
  `windows/scripts/rust/Build-Windows.ps1` is deleted by owner decision: this repo owns its
  Windows Rust build.
- **Comments are one line, only the why (2026-09-30).** The family rule, linked from
  `AGENTS.md` to [the hub's `AGENTS.md`](third_party/ANTfrastructure/AGENTS.md#comments-one-line-only-the-why):
  140 files, 5.7k lines out and 1.2k in. Traps moved to their docs pages (for example
  `crates/webgpu_renderer/docs/renderer-bounds-invariant.md`), and `AGENTS.md` gains
  *Tests and scripts that read source text*. The line-number pins in
  `buffer_desc_single_definition.rs` follow the edit (`occlusion.rs:358`,
  `histogram.rs:102`). `cargo fmt --check`, clippy `-D warnings` and the source-reading
  tests pass. ANTfrastructure moves to `497ba711` for the gate that grades every
  language and the bootstrap templates the two local copies follow.
- **`run-producer-pi.sh` runs through the image's entrypoint (2026-09-29).** All three
  `nerdctl run`s drop `--entrypoint bash`; `/hostlibs` is handed in as the caller's
  `LD_LIBRARY_PATH`, which ANTfrastructure CON23's entrypoint keeps ahead of the
  image's libcamera, behind GCC's runtime. The prologue appends the image's other
  media paths and refuses an image older than CON23. Proven under QEMU only; a board
  run is open in BACKLOG.md.
- **The chain ORT staging and every payload's proof are the hub's
  `WindowsOrtPayload.Common`** (hub `73a8dd74`, 2026-09-25), which grew out of this
  repo's own module of that name. The local copy and its Pester suite are gone;
  the cases live on in the hub's `OrtPayload.Common.Tests.ps1`. The hub's copy
  refuses an ONNX_ROOT that is not the chain install for the target arch before
  it copies anything, where the local one only checked for `bin\onnxruntime.dll`.
  `Build-Windows.ps1` names the hub commit it needs when the pin is older.
- **Build-Windows' clippy lints the CLI and the GUI crates as well as the root
  package**, with the CLI's features qualified (`kataglyphis_cli/...`). Linting the
  root package alone had left the Windows-only GUI code, which only the CLI's
  features switch on, unlinted until it reached the release build (2026-09-25).
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
- **`third_party/ANTfrastructure` moved to `d339cd52`, and the `powershell-lint`
  job became a `uses:`** — the retirement the job's own comment specified one
  pin earlier ("when a reusable lane carries this gate without a Python build
  attached"). Hub `d339cd52` added `build-python-package` to the reusable
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
  "the job stays" decision recorded in the `35960a87` row below. The
  shared-config templates did not move across this bump either, which
  `sync-shared-config.sh --repo-root . --check` confirms rather than the commit
  range being taken on trust ("Shared config in sync", both rows OK). All three
  gate jobs in `lint-gates.yml` are now one `uses:` onto ANTfrastructure.
- **`third_party/ANTfrastructure` moved to `35960a87`**, and nothing local was
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
- **`third_party/ANTfrastructure` was pinned at `3c83a5e3`**, and three local
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
- **MSIX signing works, and `Build-Windows.ps1` does it (2026-09-25).** The README's
  route ran ANTfrastructure's standalone `New-MsixPackage.ps1` over
  `packaging/msix/AppxManifest.template.xml`, but that script fills `__PACKAGE_NAME__`-style
  tokens and the template carries the `__MSIX_*__` tokens `Build-Windows.ps1` fills, so the
  manifest came out unfilled. `Build-Windows.ps1`'s MSIX step now passes the hub's
  `-Sign -SigningRoot`: a `*.pfx` at the repository root and `MSIX_PFX_PASSWORD` sign and
  verify the package, and without one it stays unsigned, as in CI. The README makes a
  test certificate with the hub's `GenerateCertificateMSIX.ps1`, and its MSIX and MSI
  paths follow `dist\windows-<arch>`. Needs the hub commit where `-Sign` takes
  `-SigningRoot`.
- **The ORT payload suite follows the hub's stricter G6 fingerprint (2026-09-25).**
  Since hub 502b9089, the census counts a source path as an ORT fingerprint only when it
  is a whole `__FILE__` string ending in NUL. The hub bump to 2fdddbc7, made for the
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
