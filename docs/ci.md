# Continuous integration

**Security audit lane.** `.github/workflows/security-audit.yml` calls the hub's reusable `security-audit.yml` on push, pull request, Mondays and dispatch. It scans `Cargo.lock` with the hub-pinned osv-scanner, records `cargo_security_checks.sh` for each `Cargo.lock`, fails a push or PR that adds a HIGH or CRITICAL advisory absent from the base commit, lists every finding on the weekly and dispatched runs, and uploads `security-audit.json`. Locally: `bash third_party/ANTfrastructure/linux/scripts/security-audit.sh`. The lint gates enforce the hub's `sha-pin` workflow convention (`workflow-conventions: sha-pin`): every third-party `uses:` names a 40-hex commit SHA.

**Python static analysis is strict.** `.github/workflows/python-static.yml` calls the hub's reusable `python-static.yml` with `strict: true` on push, PR and dispatch: one Ruff finding or ty diagnostic in a tracked `.py` file fails the lane. Locally: `bash third_party/ANTfrastructure/linux/scripts/python-static.sh --root "$PWD" --strict`. Fix a finding in the code; never add a suppression, and give a script with a shebang the executable mode.

The GPU candidate workflows accept dispatch and reusable calls; keep GPU profiling default-off and their uploads unverified until separately promoted.
The opt-in Linux AMD checked artifact requires current-attempt immutable bytes and both clean-host and GPU receipts; it does not publish a release.
Linux GPU candidate verification checks the DEB's embedded package identity, target architecture and GPU vendor version before upload.
Linux GPU candidate uploads are immutable and scoped to the run attempt; clean-host and GPU checks must select the current attempt, not a previous one.
Windows GPU candidate uploads are immutable and attempt-scoped; the hardware profile must select the current attempt's ZIP.

[AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration) lists every workflow file.
The five build lanes, one file per platform + arch, run inside ANTfrastructure images rather than on
the runner. The gate lanes pull no image, and **three of the four gate jobs run ANTfrastructure's
code**, two a reusable workflow and one its `run-pester-suite` action; the fourth runs this repo's
script suites.

| Lane (display name) | Workflow | Runs when | Image and runner |
| --- | --- | --- | --- |
| Lint gates | `lint-gates.yml` (job `lint-gates`) | every push/PR to `main`/`develop` | none — the hub's reusable `lint-gates.yml` with `ratchets: true`; the same aggregator `bash scripts/linux/run-lint-gates.sh` runs locally |
| Lint gates | `lint-gates.yml` (job `powershell-lint`) | same | none — the hub's reusable `python-ci-windows.yml` with `build-python-package: false`, `lint-powershell: true`, `lint-path: scripts`, and no `secrets:` block; `Invoke-Lint.ps1 -Path scripts -FailOnAnalyzer` on `windows-2025` |
| Lint gates | `lint-gates.yml` (job `script-tests`) | same | none — every `scripts/linux/tests/test-*.sh` on `ubuntu-26.04`, on the hub's `lib/test-harness.sh` (so the checkout takes the submodule) |
| Lint gates | `lint-gates.yml` (job `generated-artifacts`) | same | none — the hub's `shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`, then every suite in `scripts/windows/tests/` (`CargoTarget`), both under Pester 3.4.0 on `windows-2025` |
| Submodule pins | `submodule-pins.yml` | push/PR to `main`/`develop` touching `.gitmodules`, `third_party/**` or itself | none — the hub's reusable `submodule-pins.yml` (`Submodule.Pins.Tests.ps1`, Pester 3.4.0, `windows-2025`) |
| Linux x64 · build + test | `linux-x64.yml` → `reusable-linux.yml` | **every** push to `develop` and PR, and `workflow_dispatch`; on `main` through `release.yml` | family Linux CI image, inherited; `ubuntu-26.04` |
| Linux arm64 · build + test | `linux-arm64.yml` → `reusable-linux.yml` | on request: `[ci arm64]` or `[ci linux]` in a commit message (`on-request.yml`), or `workflow_dispatch` | same image (a multi-arch index); native `ubuntu-26.04-arm`, no QEMU |
| Linux riscv64 · cross build + test | `linux-riscv64.yml` → the hub's reusable `container-ci-riscv64.yml` (since 2026-10-01) | on request (`[ci riscv64]`, `[ci linux]`), and the weekly GPU run | family Linux CI image (amd64) on `ubuntu-26.04`, plus a sysroot exported from the image's riscv64 child; the tests run under QEMU |
| Windows x64 · build + test | `windows-x64.yml` → the hub's reusable `container-ci-windows.yml` (since 2026-09-25) | on request (`[ci windows]`, `[ci x64]`) | family Windows CI image, inherited; `windows-2025`, whose host runs the renderer tests and the packaged exe |
| Windows arm64 · cross build + test | `windows-arm64-cross.yml` → the hub's reusable `container-ci-windows.yml` | on request (`[ci windows]`, `[ci arm64]`) | the family image's arm64 bundle, inherited from the action's `image-arm64` default; `windows-2025` builds, `windows-11-arm` runs the product |
| Linux x64 / arm64 · clean host | `reusable-linux.yml` (job `clean-host`), in both Linux lanes | after the lane's build job | none — the hub's `clean-host-package-check` on the bare `ubuntu-26.04` / `ubuntu-26.04-arm`: the `.deb` through apt, the tarball and the AppImage, each running `--help`, `onnx-runtime` and `media-check` |
| Docs · build + publish | `docs.yml` (job `docs`) | every push/PR to `main`/`develop`, and `workflow_dispatch`; publishes on a push only | the hub's reusable `build-docs.yml` with `builder: rustdoc` and `link-allow: docs/site-links.allow`; family Linux CI image on `ubuntu-26.04` |

**A push to `develop` runs Linux x64 and the gates; every other build lane runs on
request** (owner rule 2026-10-15 for the whole family). Tokens, *Run workflow*, merging
into `main` and the release:
[the hub's `branch-and-release-strategy.md`](../third_party/ANTfrastructure/docs/branch-and-release-strategy.md).
Here `on-request.yml` offers four lanes: `[ci windows]` runs both Windows lanes, `[ci arm64]`
both arm64 ones, `[ci linux]` arm64 and riscv64, `[ci riscv64]` and `[ci x64]` one each, and
`[ci all]` all four. A change to Windows-, arm64- or riscv64-only code asks for its lane in
its commit message. `[build-arm]` and `[build-win]` are dead tokens.

**Every push to `main` is a release** (`release.yml`): all five lanes once more, then
`v<version>` with the Linux packages for both arches (each flatpak named for its arch), the
Windows MSI, MSIX and portable zip for x64 and arm64, a `SHA256SUMS` and the version's
`## [<version>]` CHANGELOG section. The version check and the release job are the hub's
[`release-publish.yml`](../third_party/ANTfrastructure/docs/branch-and-release-strategy.md#the-reusable-release-workflow),
whose collector picks the files by the `collect:` options in `release.yml`. The version lives in `Cargo.toml`'s `[workspace.package]` alone (owner, 2026-10-15:
2.3.4, and `VERSION.txt` is gone). `gh workflow run release.yml --ref develop -f
dry-run=true` is the full round a merge needs, and publishes nothing.

**File names, display names and concurrency groups are the family's**
([§ *Workflow file names and display names*](../third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#workflow-file-names-and-display-names),
[§ *How a repository implements it*](../third_party/ANTfrastructure/docs/branch-and-release-strategy.md#how-a-repository-implements-it)).
Here the steps both Linux arches share live once, in `reusable-linux.yml` ("Linux · reusable
build"), which declares no `concurrency:` group; each build workflow groups on
`<lane>-${{ github.ref }}-${{ github.workflow }}` (since 2026-10-16) and cancels a superseded
run except on the default branch. The gate lanes are "Lint gates" and "Submodule pins".

**Both Windows lanes are thin callers of the hub's `container-ci-windows.yml`** (owner
decisions 2026-09-25). Its container half is `scripts/windows/Invoke-WindowsLane.ps1`,
which a local container run executes too:

- **x64** (`windows-x64.yml`): `Invoke-DebugTests.ps1`, `Invoke-AppProfiles.ps1` (the CLI
  built and run in debug and release; its `profile` build, fat LTO plus debuginfo, ran the 16 GB
  runner out of LLVM memory in run 39471292642, so `Build-RustAll.ps1` builds it locally), then
  `Build-Windows.ps1 -SkipTests`. `Invoke-DebugTests.ps1`
  runs the workspace tests, all but the WebGPU renderer's, and builds the
  renderer's lib and 18 integration test binaries into `target\host-tests\tests.json`. The
  lane's `host-command` then runs, on the runner host, those binaries
  (`Invoke-HostTests.ps1`, through the hub's `Invoke-StagedTests.ps1 -AssertVerdict`, with
  `KATAGLYPHIS_REQUIRE_GPU=1`; it fails if the list, a binary or a passing test is
  missing, so it never reports green over nothing) and the packaged exe's `onnx-runtime`
  and `media-check` (`dist/windows-x64/bundle/kataglyphis_cli.exe`). `Invoke-DebugTests.ps1`
  fetches the model first and sets `KATAGLYPHIS_REQUIRE_MODEL=1` (2026-10-21): before that the
  real-model tests reported `ok` having skipped. They find the chain ORT through
  `$env:ONNX_ROOT\bin`, before any staging.
- **arm64** (`windows-arm64-cross.yml`): `Build-Windows.ps1`, in the family image's arm64
  bundle, then `Stage-CrossTests.ps1`, which builds the whole workspace's tests for arm64,
  renderer included, and stages them with their DLL closure, the chain ORT beside the
  binaries that load it, the model under `resources\models`, the product's GStreamer
  plugins, the renderer's fixtures and `shaders/forward/forward.slang`.
  `Initialize-BuildDirectory -Empty` clears the test destination before any copy and fails
  on a broken path; its Pester fixture runs in lint gates. The source-copy fixture also
  rejects a missing shader path.
  The hub's arch gate then grades `dist/windows-arm64`
  and the test tree, and `windows-11-arm` runs `kataglyphis_cli.exe --help`, `stats`,
  `onnx-runtime` and `media-check` natively, then every staged test with
  `KATAGLYPHIS_REQUIRE_GPU=1` (hub CON43) and `KATAGLYPHIS_REQUIRE_MODEL=1`, the renderer's
  one test at a time. The real-model tests there take the DirectML fallback below. In parallel,
  WARP itself crashes: dispatch 37422053110 (2026-10-06, `parallel-renderer-tests`) stopped
  `ibl.exe` with `0xC0000005 STATUS_ACCESS_VIOLATION`, faulting module `d3d10warp.dll` at
  offset 0x80528 (WARP 10.0.26100.9278), as run 36889467167 had stopped `headless.exe`.
  Serially all of them pass (run 36907253720), and the whole job takes 2.5 min. The fault is
  in Microsoft's driver, not this repo; re-dispatch with the input after a runner image
  update to see whether it is gone. No arm64 Windows container image exists, so that
  job is the only place an arm64 binary of this repo executes.

**Every Windows build compiles C and C++ with the image's clang-cl, never MSVC's `cl`**
(owner rule): `Invoke-WindowsLane.ps1` sets `CC` and `CXX` for both lanes, and so do
`Build-RustAll.ps1` and `Test-RustAll.ps1`, the local driver's (x64 lane since 2026-10-21,
the rest since the same day). Unset, cc-rs falls back to `cl`, which hid tract-linalg's
clang-cl break (`third_party/tract-linalg-0.23.8/PATCHED.md`). cc-rs adds `--target=<triple>`
to every clang-cl call itself (cc 1.8: `--target=aarch64-pc-windows-msvc` on the cross
lane) and archives with the `llvm-lib` beside it; tract-linalg calls `clang` on
aarch64 Windows by its own choice.

Both lanes resolve the version from `Cargo.toml`'s `[workspace.package]` (the lane's
`version-file`, read by the hub's `version_util.sh`), the only copy since 2026-10-15.
Both take the MSIX metadata and the GStreamer plugin list from `Build-Windows.config.psd1`,
and both upload `dist/windows-<x64|arm64>` whole: the portable bundle, the MSIX and the MSI.

**Every package carries its DLL closure, on both arches.** `Build-Windows.ps1` stages
beside the exe everything the exe and its DLLs import, transitively, with the hub's
`Copy-PeImportClosure`. The hub's `Get-ProductDllSearchPath` gives the search order: the
chain ONNX Runtime, then `C:\runtime\bin`, then the target's VC++ runtime. The bundle,
MSIX and MSI all ship the result.

**The exe loads the chain ONNX Runtime**, the GUI's detector included, with DirectML
registered. Before 2026-09-25 nothing in the exe called ORT, and the packages shipped none.
The chain ORT carries the DirectML EP on both arches, arm64 included (hub #113). Where no
D3D12 adapter matches, as on the `windows-11-arm` runner (`No devices detected that match
the filter criteria`, hub run 39544745432), the registration fails, the session runs on
the CPU and the detector logs `ORT DirectML provider unavailable` (2026-10-21).
The `onnx-runtime` subcommand (`ort_runtime::ensure_ort_loaded`) loads it and prints where
it came from; both lanes run it.

**Every package carries the GUI's GStreamer plugins and its model.** The camera pipeline
(`crates/gui/src/gui_wgpu/pipeline.rs`) creates its elements by name, so their plugins are
in no import table and the closure cannot find them. `Build-Windows.ps1`'s *Stage
GStreamer Plugins* step copies `Build.GStreamerPlugins` (`Build-Windows.config.psd1`) from
the image's `C:\runtime\lib\gstreamer-1.0` into `lib\gstreamer-1.0` beside any exe that
imports `gstreamer-1.0-0.dll`; a plugin the image lacks fails the step. The plugins seed the
closure, so what they import lands beside the exe, where Windows looks for a plugin's
imports. At run time GStreamer finds that directory by itself while its own DLL sits beside
the exe, and `kataglyphis_media::ensure_gst_initialized`, which the GUI calls instead of a
bare `gst::init`, points `GST_PLUGIN_PATH` at it as well. The `media-check` subcommand
builds the pipeline without starting it (no camera, no window) and fails when an element
is missing, or when an exe that carries plugins takes one from anywhere else. Where Media
Foundation is absent (Server Core has none) it builds the `autovideosrc` fallback. The model
travels in `resources\` (the *Fetch Models* step puts it there), and `resolve_model_path` takes
`resources\models\deim_dfine_n_coco.onnx` beside the exe before the compile-time checkout path, so a packaged app needs no
`KATAGLYPHIS_ONNX_MODEL`. The MSI installs the same tree as the bundle: a generated
component per file (`msi-payload-files.wxs`), placed with WiX v4's `Subdirectory`.

**The tests' plugins are the product's plus `Build.TestGStreamerPlugins`.** On the arm64
device the tests run against what `Stage-CrossTests.ps1` copied beside them, not the image's
runtime, so the cat cam's capture tests need their own: `videofilter` for the `videoflip`
that is always in the capture since its turn became a run-time setting, `multifile` and
`jpeg` for the image source. Its rotation commit (d1d6e37) failed arm64 alone for that,
while x64, which tests inside the image, had them all. The product bundle stays as it was.

On a cross build the script also:

- builds with `--target aarch64-pc-windows-msvc` and `PKG_CONFIG_ALLOW_CROSS=1`
  (gstreamer-sys asks pkg-config; the bundle's `.pc` files are arm64);
- runs clippy for aarch64, and leaves audit/deny and fmt to the x64 lane, which
  grades the same commit; the tests it only builds, for `windows-11-arm` to run.

Every arch-dependent path and name comes from `Get-CargoTargetLayout`
(`scripts/windows/modules/WindowsCargoTarget.Common.psm1`, pinned by
`scripts/windows/tests/CargoTarget.Tests.ps1`). Locally, arm64 is the x64 lane's
container run with the arm64 bundle's reference
(`bash third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh --windows-arm64`)
and `Invoke-WindowsLane.ps1 -TargetArch arm64`.

**"Inherited" is literal: no build workflow names an image.** Both old ones opened with
a `CONTAINER_IMAGE:` env entry holding the full reference and handed it to every
container step: a copy of ANTfrastructure's `versions.env` value that a fleet-wide tag
bump would leave behind. Every step now omits the `image:` input and takes the container
actions' default, which `verify_ci_image_refs.py` grades against `versions.env` on
ANTfrastructure's own build (check A); check D fails this repo's lint gate if the
reference is re-typed into a workflow, a script, or a comment.

**PowerShell is graded too, with the analyzer enforcing.** The job is a `uses:` since 2026-09-15: hub `d339cd52` gave `python-ci-windows.yml` a `build-python-package` input (boolean, default true) gating its container build and made `GHCR_PAT` optional, so a Rust crate can take the family's PowerShell lint without a Python package build. `Invoke-Lint.ps1` makes two passes: a parse + AST-trap gate that is always fatal, and PSScriptAnalyzer. The sibling consumer this job is modelled on leaves the analyzer advisory because it has untriaged findings; this tree has none (measured 2026-09-15: 0 errors, 0 warnings over all 9 files), so `-FailOnAnalyzer` is passed and the ratchet is set at the number this repo actually has. `-Path scripts` is walked recursively, and the hub's `PSScriptAnalyzerSettings.psd1` is resolved against the *script's* own location: the ruleset is consumed by reference and nothing is copied in here.

**PowerShell complexity is strict on every push and PR.** `powershell-quality.yml` calls the
hub's reusable scanner over `scripts` with `strict: true`, uploads JSON from ignored `logs/`, and
keeps parser and analyzer failures fatal. A unit over 15 cyclomatic, 5 nesting or 80 function lines
fails the job, with no exception list: `Build-Windows.ps1` runs each build step from a named
function for that reason. Split code into functions; never raise a limit. A manual dispatch without
`strict` only measures.

**Nothing generated may be tracked.** The hub's `shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1` runs `Get-TrackedIgnoredFile` and `Get-TrackedGeneratedArtifact` over this root; only the generated-output pathspecs are local, one per line in `scripts/windows/generated-artifacts.txt` (since 2026-10-19; a local copy of the suite held them before). Locally, from the repo root: `Invoke-Pester -Path third_party/ANTfrastructure/shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`. A `.gitignore` rule stops a file from being *added* and does nothing once a path is in the index, which is why `git rm --cached` has been needed twice here (build logs under `logs\windows\`, and the flatpak repo). Mind the pathspec form: `'**/__pycache__/*'` matches, `'**/__pycache__/'` matches **nothing**, and the suite refuses it.

**The lint lane runs with `--ratchets` on** since 2026-09-15. On top of the six always-on gates that adds the docs cross-reference gate plus eight measurement gates (code size, complexity, dead functions, comment size, stdout returns, masked declarations, trailing conditionals, and a shellcheck *warning* ratchet), each graded against a freeze file at the repo root. Three of them carry rows (`comment-size.allow`, `code-complexity.allow`, `dead-functions.allow`), seeded from the first run; the rest are absent, which the gates read as a zero baseline. **The contract is two-way**: a new offender fails, and so does an entry that is no longer over the limit, so fixing one means deleting or updating its row in the same change. The docs gate has no freeze file and never will: a `docs/…md` pointer in code either resolves from the repo root (or, when it starts `../`, from the file) or it is a finding.

## The riscv64 lane

**Cross-built on amd64, tested under QEMU** (owner decision 2026-10-01, hub CON48); why,
the sysroot, the wrappers and QEMU are
[the hub's `riscv64-cross-test-lanes.md`](../third_party/ANTfrastructure/docs/riscv64-cross-test-lanes.md).
`linux-riscv64.yml` is a thin caller of the hub's `container-ci-riscv64.yml`; its container
half, `bash scripts/linux/ci-container-steps.sh riscv64-test`, runs `cargo test --workspace
--locked --target riscv64gc-unknown-linux-gnu` after `riscv64_cross_env`.

- **What runs:** every workspace test, 720 of them (measured 2026-10-20 with the GUI, the burn
  demos and the webcam engine in: 8 min 33 s for the whole step on a warm cargo home; 358 and
  2 min 52 s on 2026-10-01). `observant` links the image's riscv64
  GStreamer, the inference tests the riscv64 ORT loader, and the CLI integration test spawns
  the riscv64 `kataglyphis_cli` through binfmt. Since 2026-10-15 the step fetches the model
  and sets `KATAGLYPHIS_REQUIRE_MODEL=1`, as the Linux `test` step does: before that the
  real-model tests reported `ok` having skipped, since `cargo test` hides a passing test's
  `SKIP:` line.
- **The text contract runs apart, on scalar MLAS.** The workspace run takes `-- --skip
  every_contract_case_decodes_to_its_expected_ids`, then the step runs `--test contract` with
  `ORT_MLAS_RISCV_FORCE_SCALAR=1` and `KATAGLYPHIS_REQUIRE_GENAI=1`. It still decodes the
  exact token ids on riscv64 under QEMU, but it does not exercise the RVV `MatMulNBits` kernel:
  ORT 1.31.0 segfaults there on SmolLM2's 4-bit weights. `MatMulNBits::UseSharedPrePackedBuffers`
  guards its 4-bit exception with `MLAS_TARGET_ARM64` alone, so on riscv64 it sets
  `scales_are_packed_` although the scales were never packed, and the kernel reads
  `QuantBScale == nullptr`. Neither a larger stack nor another vector length helps.
  Every other test stays on RVV. The fix and its limits: `BACKLOG.md`, **RVV `MatMulNBits`**.
- **What skips:** the renderer's GPU rendering. The step disables every Vulkan driver
  (`VK_LOADER_DRIVERS_DISABLE='*'`) and leaves `KATAGLYPHIS_REQUIRE_GPU` unset, so
  `headless_or_skip()` returns `None` and the CPU-side assertions still run. lavapipe does
  work under QEMU: `RISCV64_GPU_TESTS=1` sets `KATAGLYPHIS_REQUIRE_GPU=1`, and the whole
  workspace then passed in 16 min 55 s on a 32-core host (the headless suite alone: 38
  tests), too long for a 4-vCPU runner on every push.
- **The GPU suites run weekly instead** (owner decision 2026-10-01): a `schedule` run every
  Sunday 02:17 UTC, and any manual run with the `gpu-tests` input, set
  `RISCV64_GPU_TESTS=1` and a 120-min timeout. Measured on GitHub (run 36885386597): 358
  passed, 0 failed, no `SKIP: no GPU`; the test step took 43.5 min, the job 56 min.
- **Locally:** build the sysroot once with the hub's
  `linux/scripts/02-toolchain/riscv64-sysroot.sh`, mount it at `/opt/riscv64-sysroot`, and
  run the same step in the family image (the hub page has the command). The step prints
  `unstable feature specified for -Ctarget-feature: v` per crate: the image's riscv64 Rust
  flags, not a fault.
- **Not on a 16 KiB-page host** such as a Raspberry Pi 5: the cat cam's capture tests time
  out there, since no ORC-linked GStreamer plugin loads (2026-10-13). Trust them only from a
  4 KiB-page host such as the CI runner;
  [the hub's traps](../third_party/ANTfrastructure/docs/riscv64-cross-test-lanes.md#traps-each-found-by-a-failing-run)
  say why.
- **Keep pkg-config on the riscv64 sysroot** (hub CON130). `riscv64_cross_env` removes the
  amd64 image's `PKG_CONFIG_PATH` before gstreamer-sys runs, and the hub's GStreamer cross smoke
  rejects an x86_64 link path. The earlier bad path was
  `/opt/riscv64-sysroot/opt/gstreamer/lib/x86_64-linux-gnu`, which has no riscv64 libraries.
- **GStreamer loads only riscv64 plugins.** The image's `GST_PLUGIN_PATH` starts with the amd64
  plugin dir, which QEMU opened on the host (1224 warnings in run 40837860172);
  `riscv64_cross_env` drops it since hub 0aef805d.
- **The root crate's real-model tests take turns** (`model_turn` in `src/lib.rs`, 2026-10-20): on
  four cores three model loads at once under QEMU kept the webcam engine's first event past its
  60 s (run 39487523314); one at a time all 25 pass in 168 s on 4 CPUs locally.
- **A spawned host program is not remapped** (`execve`, the same traps). A bare `sleep`
  exited at once there, which made a test of
  `camera::run_with_timeout` see output instead of a timeout (9e0a401 to 8a5337b, red); the
  test now goes through `sh -c`, and the function returns `None` for any failed exit.

## Lane facts that cost debugging time

- **The arm64 lane runs natively on `ubuntu-26.04-arm`**, on every push from 2026-09-24 to 2026-10-15 and on request since. It was opt-in via `[build-arm]` for runner minutes before that, never because it could not pass: `:latest` (then `:latest-cross`) has been a multi-arch index since 2026-09-04.
- **Linux artifacts are named by `VERSION`, not by `github.ref_name`.** On a pull request the ref name is `<number>/merge`, `upload-artifact` refuses a `/` in a name, and every PR run of the Linux lane went red at *Upload all artifacts* with the build, tests and package green (run 35752031200, 2026-09-22). Fixed on 2026-09-24, when the lane started running on every PR on both architectures.
- **Every cargo step of both Linux lanes is one named step of one script**, `scripts/linux/ci-container-steps.sh` (`ort-chain-only`, `debug`, `security`, `fmt-clippy`, `test`, `coverage`, `bench`, `release`; `docs` builds locally what `docs.yml` builds). Each step used to inline its own `bash -lc 'set -e; git config --global --add safe.directory /workspace; bash third_party/.../cargo_<x>.sh'`, the same prologue eight times. Reproduce any step with `bash scripts/linux/ci-container-steps.sh <step>` inside the image; `reusable-linux.yml` runs exactly that line. The one container step outside it is packaging, `scripts/linux/package-linux.sh`, which runs `--privileged` for flatpak-builder. Until 2026-10-20 `linux-x64.yml` also checked six feature sets in a `feature-matrix` job; with no features left, the lane's own build covers them.
- **The docs site is `docs.yml`'s, not a lane's** (2026-10-19). It calls the hub's `build-docs.yml` with `builder: rustdoc`: `cargo_build_doc.sh` documents all 13 crates into `target/doc`, the hub renders the `guides` (every `docs/` page, since 2026-10-21) beside `oxidant/index.html` and lists them atop each page's sidebar, adds its `.htaccess` and 404 page, the built-site link gate checks every internal link and anchor, and a push to `develop` or `main` publishes to <https://rust.jonasheinle.de> under a never-cancelled group. A red lane no longer holds the site back; a dead link fails it. The 24 keys in `docs/site-links.allow` are rustdoc's copies of relative links in blanket impls' docs (`tracing`'s `WithSubscriber`, burn's `Module`); the freeze is two-way, so a key that resolves again must go. Inputs, secrets and the gate: [the hub's `docs-sites.md`](../third_party/ANTfrastructure/docs/docs-sites.md).
- **The container runs as uid 1001, not root.** `apt-get` fails with `Permission denied`, so a workflow step cannot install system packages: whatever the image lacks, it lacks. The image's `CARGO_HOME` (`/usr/local/cargo`) and `RUSTUP_HOME` belong to uid 1001, which the hub's consumer-image contract requires (row 2 of `docs/consumer-image-contract.md`): `find /usr/local/cargo /usr/local/rustup ! -uid 1001` finds nothing in `:latest`, and a `cargo check` fills the registry there (2026-10-19). The lane jobs set `-e CARGO_HOME=/tmp/cargo-home` only because their cache mount keeps the registry there between steps.
- **The Windows x64 lane pins its cargo tools** (the arm64 cross build runs no audit/deny). `scripts/windows/Build-Windows.ps1` installs cargo-audit and cargo-deny with `--version`, read from `$env:CARGO_AUDIT_VERSION` / `$env:CARGO_DENY_VERSION` (baked into the image) and falling back to the submodule's `versions.env` through `ConvertFrom-VersionsEnv`; unresolvable **throws**. Unpinned, `cargo install` takes whatever crates.io serves that minute, so a new advisory-db schema turns the lane red with no commit to bisect. No try/catch wraps the install any more either: a swallowed failure left both gates running whatever was on PATH.
- **`deny.toml` `[graph].targets` lists every target this repo builds**: Linux x64, arm64 and riscv64, Windows x64 and arm64 (`x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`), Android (`aarch64-linux-android`, Cargokit's only ABI here) and `wasm32-unknown-unknown`. cargo-deny checks only the dependencies of the targets listed, so a `cfg(windows)`, Android or wasm crate is otherwise never audited. A new build target joins the list in the same change; a finding on it is fixed by an upgrade or removal, not a new ignore, skip, exception or license.
- **`cargo_security_checks.sh` is a gating step, not an advisory one.** The *Security checks (cargo audit + cargo deny)* step in `reusable-linux.yml` (so in both Linux lanes) runs ANTfrastructure's `linux/scripts/02-toolchain/rust/cargo_security_checks.sh`, and a finding fails the lane. It reads **two** ignore lists: `.cargo/audit.toml` (cargo-audit) and `deny.toml` `[advisories].ignore` (cargo-deny). For a crate that is built they hold the same ids, since an id on one list only leaves the other tool reporting it; paste and ttf-parser are the exception, built but no longer reported by cargo-deny 0.20.2 (2026-10-20), so `audit.toml` alone lists them. Cargo-audit also reads optional dependencies that no workspace feature enables. The documented `burn-dispatch` patch removes the unused Torch/ZIP/Zstd branch entirely from `Cargo.lock`; keep it out until upstream offers an equally safe graph. Prefer an upgrade or removal over an ignore: `chacha20` 0.10.1 (yanked) and `stable-vec` 0.4.2 (unsound) both had one, so neither is on either list.

The `lint-gates.yml` script-test job must run `test-burn-lock-graph.sh`: it rejects reintroduced
Torch/ZIP/old Zstd packages from `Cargo.lock` and mutates the lock to prove the verdict can fail.
Keep Burn's exact root version, the vendored dispatch version and Renovate's review rule together;
the local Renovate wrapper rechecks the graph after `--apply`.

Lint the workflows locally with the submodule's pinned, SHA-verified actionlint (works from Git Bash on Windows):

```bash
bash third_party/ANTfrastructure/linux/scripts/lint-workflows.sh .
```

The trailing `.` is load-bearing: without it the script lints ANTfrastructure's own workflows instead of this repo's, and reports green either way.
