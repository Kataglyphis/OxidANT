# AGENTS.md

Guidance for AI agents (and humans) working in OxidANT.

**Reuse before adding Bash or PowerShell code.** Search this repository and every direct
submodule, including `third_party/ANTfrastructure`, for an existing implementation. Call or
extend its canonical owner, passing project-specific values as arguments. For new behavior
needed by multiple consumers, create or extend one shared owner module and call it here.
Keep unique product logic in this repository.

Laid out on ANTfrastructure's six-section template
([`third_party/ANTfrastructure/shared/templates/AGENTS.md.template`](third_party/ANTfrastructure/shared/templates/AGENTS.md.template)),
the same shape as OrchestrANT, AccelerANTgine and ANThology. Where a paragraph goes:
*would this still be true in a different project?* If yes, ANTfrastructure owns it and
§ 2 links to it. If no, § 4 writes it out.

**Dated history is not here.** Measured baselines and the post-mortems of the 2026-08-07
packaging, graphics-stack and tract migrations live in [`CHANGELOG.md`](CHANGELOG.md).
This file is what to do now; that file is what happened when.

## 1. What this project is

The Kataglyphis family's **Rust workspace**: the crates two other repositories build as
a submodule, plus the gates, packaging and practices they inherit with them. It started
as a project template and the scaffolding is still here, but the crates have real
consumers, which makes renames expensive ([Consumers](#consumers)).

The root `Cargo.toml` is both the workspace and the root package `oxidant`: a lib with
`cdylib`/`staticlib`/`rlib` crate types. **No crate has a Cargo feature** (owner,
2026-10-20): every target builds what it can run ([§ Targets and their system
dependencies](#targets-and-their-system-dependencies)). Every crate, what `src/` and
`tests/` hold, and the three documented vendored crates under `third_party/`:
[`docs/crates.md`](docs/crates.md). The crates are `crates/burn_demos`, `crates/cli`, `crates/core`,
`crates/cxx_bridge`, `crates/esp32`, `crates/esp32-protocol`, `crates/gui`, `crates/inference`,
`crates/media`, `crates/observant`, `crates/telemetry`, `crates/text` and `crates/webgpu_renderer`;
the root package's modules are `src/api` (the flutter_rust_bridge surface) and `src/platform`. The tooling submodule, `third_party/ANTfrastructure`,
is **the ground truth for every container and PowerShell concern**; read § 2 before
writing any helper.

### Consumers

Repositories that build this one as a submodule. A rename or a `[lib] name` change goes
into each of them **in the same change**:

- [OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT): the root package
  through flutter_rust_bridge (Cargokit, podspecs, the committed `frb_generated.dart`
  loader stem), and `crates/observant` for its Stream page.
- [BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett):
  `crates/cxx_bridge` through Corrosion, and `crates/webgpu_renderer` through plain cargo
  (its wasm32 demo for the docs site, its test suite and the wasm size budget:
  `scripts/linux/{docs-build-web,run-cargo-tests,wasm-size-budget}.sh` there). The import
  is in `Src/CMakeLists.txt`: `corrosion_import_crate(MANIFEST_PATH
  ../third_party/OxidANT/Cargo.toml CRATE_TYPES staticlib CRATES kataglyphis_cxx_bridge)`,
  then `corrosion_add_cxxbridge(oxidant_bridge ...)` over the crate's `src/lib.rs`, so its
  package name, the **staticlib** and the file name are part of that repository's build.
  The bridge is a crate of its own: the root package links GStreamer and
  the ORT loader, which the C++ engine would have linked and shipped for one function.

## 2. What ANTfrastructure owns — links only

Containers, Dockerfiles, CI plumbing and PowerShell belong to the submodule. **Search it
before writing a helper**, and do not re-derive host knowledge here: Stevedore, Rancher
Desktop, wcifs/bindFlt and the container hosts are written down there, in more depth than
this file should carry. Start at
[`third_party/ANTfrastructure/docs/INDEX.md`](third_party/ANTfrastructure/docs/INDEX.md),
which maps topic → owning document, so one hop survives upstream reorganisation.

| Question | Document |
| --- | --- |
| How do I set up a Windows host for Stevedore? (services, `docker-users`, CNI nat conf, pwsh, gates) | `docs/windows-host-setup.md` |
| Why does a layer commit fail / what is process isolation doing? wcifs, bindFlt, the `ActivateLayer 0x20` bug | `docs/windows-builds.md` |
| Dev Drive filter setup, bind mount vs tar-pipe, container reuse | `docs/windows-container-build-performance.md` |
| How do my builds share sccache with the other build hosts? The host's dufs plus a shared S3 bucket, the key by name on the reused container's `docker exec`, the fallback when the bucket stalls, and the bucket's server runbook | `docs/windows-build-resources.md` § *Two cache levels*, `linux/sccache-s3/README.md` |
| How do I run Linux containers on Windows? | [`docs/rancher-desktop-linux-containers.md`](third_party/ANTfrastructure/docs/rancher-desktop-linux-containers.md); this repo's command and its traps: [`docs/getting-started.md`](docs/getting-started.md#linux-containers-locally-rancher-desktop) |
| Which image, which tag, which engine? | `docs/adopting-in-a-new-project.md` |
| Which lanes run when, how to ask for one (`[ci windows]`, `[ci arm64]`, `[ci all]`), and when `main` is released | `docs/branch-and-release-strategy.md` |
| Why did my lane not run? | `docs/ci-build-triggers.md` |
| How do I upgrade a dependency, and what will `--apply` refuse to move? | `docs/dependency-updates.md` |
| How long may a code comment be? One line, only the why; API docs short; gated | [the hub rule, Comments](third_party/ANTfrastructure/AGENTS.md#comments-one-line-only-the-why) |
| Searching the tree: `rg`, not `grep -r` | [the hub rule, Searching](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/AGENTS.md#searching-the-tree-ripgrep-rg) |

**When this file contradicts one of those, the submodule wins.** It happened twice, both
times because a procedure was retyped here instead of linked: this file once claimed Dev
Drive volumes *refuse* bind mounts (they work; create-then-rename fails), and it carried
an `fsutil devdrv` command that lacked `/volume` and split its filter list on a space, so
it could never have worked. That is why § *Build & test in the Stevedore Windows
container* links rather than restates.

### Reach for these before writing a helper

The table of hub helpers this file used to carry (each one written here first and later
found upstream) is the hub's own now, with its two caveats:
[`docs/adopting-in-a-new-project.md` § *Reach for these before writing a helper*](third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#reach-for-these-before-writing-a-helper).
Read it before writing a PowerShell function, a cargo step or a CI action. The MSI step
is a hub call too: `Invoke-MsiPackage` in `WindowsMsix.Common`, first called from this
repo.

Nothing here needs Windows PowerShell 5.1 semantics: every script carries
`#requires -Version 7.0` and CI invokes `pwsh`.
The arm64 Cargo staging suite passes its script and reset callback to ANTfrastructure's
shared stage-directory fixture; keep its checks for cleanup and failed allocation armed.
Its source-copy suite also uses the hub's parsed loop and fixture helpers; keep the
shader-path assertions and removed-shader mutant here. The Windows build driver uses
`Get-BuildWorkspaceConfig` for config and workspace-root selection, then validates the path here.
`Stage-CrossTests.ps1` also stages `shaders/forward` beside the mirrored renderer sources:
the native shader-constant tests read `forward.slang` through embedded `CARGO_MANIFEST_DIR`.
Both Windows build lanes request direct submodules to avoid the hub's cyclic
documentation subtree during the short-path checkout.

## 3. Critical invariant: submodule pins

Builds are only supported against the **recorded submodule gitlink**, the commit CI
builds green; `git submodule update --checkout --recursive` restores it. If a drifted
submodule is what you want, move the gitlink **and** fix the fallout in the same change;
do not fork upstream behaviour locally.

**The `.gitmodules` url is `https://`, never `git@github.com:`**: a recursive checkout of
either of the [Consumers](#consumers) inherits it. Why, and how to change one:
[the hub's § *Submodule URLs are https, never `git@`*](third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#submodule-urls-are-https-never-git).

The tooling submodule, `third_party/ANTfrastructure`, supplies every gate: the
shellcheck/actionlint/gitleaks binaries the lint lane bootstraps, the cargo, packaging
and docs drivers, the Windows modules and the ORT census (G6). A drifted gitlink silently
changes every gate, and `git submodule status` marks it only with a `+`, easy to miss in
CI output. The images are the exception: the lanes take them from the hub's actions and
reusable workflows at `@develop`, which read `versions.env` at that ref, not at the pin.

**Guarded by the hub's `Submodule.Pins.Tests.ps1`**, run after any pin bump from
[`.github/workflows/submodule-pins.yml`](.github/workflows/submodule-pins.yml): every configured
submodule is checked out, at its recorded commit, and pinned to a commit its remote still has. It
stays separate from `lint-gates.yml` because the Pester pin (`3.4.0`) has no Linux release.

**Version couplings with the hub:** the toolchain (`RUST_VERSION`) and the two
cargo-tool pins (`CARGO_AUDIT_VERSION`, `CARGO_DENY_VERSION`) are read from
`third_party/ANTfrastructure/linux/scripts/01-core/versions.env` at run time, not copied,
and no CI image reference is written here: `verify_ci_image_refs.py` check D fails the
lint gate on one.

## 4. Pitfalls specific to this project

Everything here is false or meaningless in another repo, so it is written out rather
than linked.

### The trap a Windows checkout hits before anything runs

- **A CRLF checkout breaks it before anything runs**: bash in the container stops at `set: pipefail\r: invalid option name`. `.gitattributes` pins `*.sh` (and the workflows and `models.lock`) to LF; an older checkout needs the one-time fix in [the hub's failure-modes.md](third_party/ANTfrastructure/docs/failure-modes.md#set-pipefailr-invalid-option-name).

### The bin was renamed, not the lib — do not undo it

A bin named `oxidant` would collide with the root lib on `oxidant.pdb`
([rust-lang/cargo#6313](https://github.com/rust-lang/cargo/issues/6313)), and `[lib] name` decides
file names the [Consumers](#consumers) build against: any rename moves them in the same commit.
`BINARY_FILE` is the cargo artefact, `BINARY` what a user invokes. Every package carries its runtime
closure, its model, the project's `LICENSE` and `licenses/cargo/`; never write a notice by hand:
[`docs/packaging.md`](docs/packaging.md#the-bin-was-renamed-not-the-lib--do-not-undo-it).

### Inside the Stevedore Windows container

Sources and artifacts travel by tar-pipe, cargo's trees stay outside the workspace, and everything
is `pwsh`: [`docs/windows.md`](docs/windows.md#inside-the-stevedore-windows-container).

### `cargo_fmt_clippy.sh`

**The `fmt-clippy` step runs ANTfrastructure's `cargo_fmt_clippy.sh`** like every other
cargo step; it was the one that did not delegate until 2026-09-15. Both reasons this repo
hand-rolled `cargo fmt` / `cargo clippy` were fixed upstream first: the driver's old first
line `rustup component add rustfmt` exited 127 on an image without rustup (it probes
first now; its header says PROBE, DO NOT ADD), and `--all-features` was hard-coded at its
line 40 (now the `CARGO_CLIPPY_ARGS` knob). That exit 127 was masked by
`continue-on-error: true` for months and let an entire crate reach the default branch
unformatted and with 12 clippy errors; the step gates now.

- **Scope:** `scripts/linux/ci-container-steps.sh` sets `CARGO_CLIPPY_ARGS` to
  `--workspace --locked`, the whole workspace with nothing left out.
- **Positional arguments reach `cargo fmt` only**, not both tools.
- **The step first fails a stale `src/frb_generated.rs`.** `scripts/linux/frb-codegen.sh` is the
  one flutter_rust_bridge input and flag set, also used by OmniAccelerANT's generator; regenerate
  there after changing `src/api`: [the bridge bindings](docs/testing.md#the-bridge-bindings).

### Never expand a manifest template with `-replace`

PowerShell treats the replacement side as a substitution template, so a value containing `$&` re-inserts the whole matched token. A description of ``Renderer $& x`` produced `Desc="Renderer __MSIX_DESCRIPTION__amp; x"`, the literal token, shipped into the manifest. `Expand-XmlTemplateTokens` uses an ordinal `[string].Replace` and escapes each value itself.

### Targets and their system dependencies

**No crate has a Cargo feature, and none may get one** (owner rule): every
dependency is unconditional or `[target.'cfg(...)'.dependencies]`, every gate in the code is
a target `cfg` or the root's `cfg(desktop)` (Linux, Windows, macOS, set by `build.rs`). A
`--features` on the command line fails: *"the package 'oxidant' does not contain these features"*.

| Target | What builds | Needs |
| --- | --- | --- |
| Linux x64/arm64 (`:latest`), riscv64 (cross, the image's sysroot), Windows x64/arm64 (`:winamd64`, `:winarm64`) | The whole workspace: tract plus the chain ORT with CUDA registered (DirectML on Windows), GStreamer capture, the webcam engine, the wgpu GUI, the ESP32 bridge, the burn demos | GStreamer's development files (source-built into `/opt/gstreamer` on `PKG_CONFIG_PATH`; never the distro's `libgstreamer*-dev`, which the image purges), the chain ORT at run time |
| `aarch64-linux-android` (OmniAccelerANT's Cargokit) | `oxidant` alone: the detector on tract; the webcam and ESP32 calls answer that the target has neither | The NDK: no GStreamer, serial port or chain ORT exists there |
| `wasm32-unknown-unknown` (OmniAccelerANT's web build) | `oxidant` alone, without the detector, whose model loads from a file the web cannot have; tract built but took the release wasm from 0.66 to 15.5 MB | The wasm target |

- **ORT is `load-dynamic` everywhere** and `download-binaries` (pyke's prebuilt ORT, plus
  `openssl-sys` for its TLS) is banned by the owner rule of 2026-09-23;
  `scripts/linux/check-ort-chain-only.sh` gates it in CI. At run time
  `crates/inference/src/ort_runtime.rs` finds the chain ORT (`ORT_DYLIB_PATH`, the exe's
  directory and its `lib/`, then the image prefix: `/usr/local/lib/onnxruntime-cpu/lib`,
  `$env:ONNX_ROOT\bin`; never a bare-name load) and refuses a file without the chain's
  source path.
- **tract is the fallback, and says so.** With `KATAGLYPHIS_ONNX_BACKEND` unset the detector
  loads ORT; when that fails it warns `Falling back to tract: <ORT's reason>` and loads tract.
  `ort` refuses the fallback, `tract` skips ORT. On Android tract is the backend.
- **Both GUIs build on Linux.** GTK4 >= 4.10 is supplied by the family image;
  `gui-gtk` retains its bus watch until the session ends. The Linux test step uses
  Xvfb and requires the GTK lifecycle test; Linux packages carry the GTK closure and
  `gtk4paintablesink`. Commands: [running.md](docs/running.md#the-gtk4-camera-gui-linux).
- **The camera GUI's wgpu has no GL backend** (`crates/gui/Cargo.toml`): its `GpuBackend`
  never picks GL, and the `opengl32.dll` import GL brings stops the CLI from loading in
  Windows Server Core ([docs/testing.md](docs/testing.md#on-windows-the-renderers-tests-run-outside-the-container)).

A plain Ubuntu box (e.g. the WSL recipe below) *does* need the distro packages, because nothing there provides the source-built stack. That difference is why "install the -dev package" is the wrong instinct when the image is involved.

### Known gaps

- **On arm64 the renderer tests run llvmpipe at 128-bit vectors**. The image sets `LP_NATIVE_VECTOR_WIDTH=256` on every arch for Mesa's BVH sort (hub CON44), and at that width LLVM's AArch64 JIT killed a renderer test binary in about one arm64 run in five: `LLVM ERROR: Do not know how to split this operator's operand!` (SIGABRT; runs 37367760093, 37382902869, 37005853556) and a SIGSEGV in `forward_ambient` (run 37428962397). Nothing here uses ray tracing or subgroups, so `scripts/linux/ci-container-steps.sh` exports 128, arm64's native width, for every step on an aarch64 host.
- **The Linux lanes render the golden tests on a software Vulkan device.** The
  image supplies lavapipe on amd64 and arm64. `KATAGLYPHIS_REQUIRE_GPU=1` fails rather
  than silently passing a test that found no adapter; Windows uses WARP. The formerly
  ignored nonuniform-instance normal test is active after correcting the Slang matrix
  orientation. Its pixel threshold is unchanged. [testing.md](docs/testing.md#the-gpu-tests-must-render).
- **OxidANT owns `shaders/` and its checked-in generated WGSL.** Regenerate through
  the hub compiler and run `compile-slang-shaders.sh --check`; do not hand-edit WGSL.
  BeschleunigerBallett consumes the shared mathematics at its recorded OxidANT pin.
  Ownership and regeneration: [shaders.md](docs/shaders.md).

### Shared inference and texture contracts

`third_party/ModelANT` supplies test-only preprocessing, decode and text-generation references; model weights still
come from `models.lock`. Keep its gitlink aligned with the C++ consumer when changing
the contract. The one-entry model cache returns a stable reference for each request,
so replacement by another model path cannot redirect an in-flight call.

`abi/texture.json` generates `include/kataglyphis_texture_abi.h` and
`src/texture_abi.rs`. Run `scripts/generate-texture-abi.py --check`; a contract change
regenerates both files and updates Omni's pinned header. Rust checks the loaded
`knt_api_version` before resolving `knt_push_frame`. A breaking signature requires a
version bump and coordinated consumer changes. [testing.md](docs/testing.md).

**Detection API and execution providers.** `detect_rgba`, `detector_labels` and
`WebcamStreamConfig.classes` take class ids; keep `detect_persons_rgba` and
`start_webcam_inference` as wrappers and rename nothing named `person_*`: OmniAccelerANT binds
them. The class names live once, in `kataglyphis_core::coco`. Every `KATAGLYPHIS_ORT_DEVICE`
other than `cpu` and `auto` (including `xnnpack` and `dnnl`) fails with `ProviderUnavailable`
when its provider is missing or cannot load; never add a CPU fallback for a named provider.
`migraphx` is AMD's plugin module, one row in `crates/inference/src/ort_plugin.rs`: registered by
path once per process as `MIGraphX`, then exactly one of its devices appended with `device_id`
alone. Never the by-name append, never an option the plugin does not list, never an unregister;
another plugin is one more row ([running.md](docs/running.md)).
The test step's `check-backend-parity.sh` compares ORT against tract, XNNPACK and DNNL on
ModelANT's photo: [running.md](docs/running.md), [testing.md](docs/testing.md).

### Tests and scripts that read source text

- `crates/webgpu_renderer/tests/buffer_desc_single_definition.rs` pins the two `BufferDescriptor` outliers by
  line number (`occlusion.rs:358`, `histogram.rs:102`), so a comment edit above either line breaks it.
- `TEXTURE_2D_SHAPE_OK` must stay on the first line of its `wgpu::TextureDescriptor {` body
  (`tests/texture_desc_single_definition.rs`).
- `scripts/linux/check-ort-chain-only.sh --help` prints lines 2-10 of the file (`sed -n '2,10p'`).
- The two bootstrap copies (`scripts/linux/lib/antfrastructure.sh`,
  `scripts/windows/Resolve-BuildModule.ps1`) and exact Renovate Git-mode companion
  (`scripts/linux/lib/renovate-git-mode.sh`) come from the hub. `sync-shared-config.sh --check`
  guards all three; re-copy them after an upstream change.

### Verifying locally on a Windows box (no MSVC required)

Without MSVC nothing links, and `link: missing operand` is the coreutils `link` on PATH, an
environment fault. Verify in WSL instead:
[`docs/getting-started.md`](docs/getting-started.md#verifying-locally-on-a-windows-box-no-msvc-required).

## 5. Build, run, test

`bash scripts/linux/quality-report.sh` measures this checkout into `logs/code-quality/measurements.json`
and `queue.md`; `--compare <file>` or `--compare previous-develop` grades it (exit 0 ok, 1 regressed,
2 incomplete). `quality-report.toml` declares this repo's lane outputs and metrics, the coverage and
rustdoc-warning ones from the x64 lane's `lane-reports-ubuntu-26.04-x64` artifact: hub
`docs/code-quality-tooling.md` § quality-report. A regressed verdict on develop is fixed or recorded
in `BACKLOG.md` before new feature work. `bash scripts/linux/quality-report.sh --perf-host` runs the
latency commands that toml declares inside `:latest` and grades them against `perf/baseline-<cpu>.json`;
it refuses a dirty tree, a loaded host or a running image chain. It times two-cats p50/p95 per CPU provider (CPU, XNNPACK; DNNL reports not collected while the image
lacks `libdnnl.so.3`) through `kataglyphis_cli detect-latency`, and SmolLM2-135M time to first token and decode
rate through `crates/text`; run it as a
`systemd-run --user` unit, and `--update-baseline` rewrites the baseline: hub `docs/fleet-health.md` § perf-host.

```bash
cargo build --workspace --locked                      # dev/debug
cargo build --workspace --locked --profile profile    # custom: release + debuginfo
cargo build --workspace --locked --release            # fat LTO, codegen-units 1, panic=abort, stripped
cargo build --locked --profile observant -p kataglyphis_observant  # the camera service: release, but a panic unwinds
cargo test  --workspace --locked                      # unit + integration + proptest fuzz + doc tests
cargo run --bin kataglyphis_cli -- --help             # the CLI
cargo run -p kataglyphis_burn_demos --bin burn_demos  # the burn demos
```

**riscv64 runs the text contract on scalar MLAS alone** ([`docs/ci.md`](docs/ci.md#the-riscv64-lane)): ORT 1.31's RVV `MatMulNBits` segfaults on the 4-bit model. Keep the detector suites on RVV, and remove the override when the chain's ORT carries the fix.

**Text generation keeps its contract** ([`docs/text.md`](docs/text.md)): `seed`, `json_schema` and `stop` reach both backends; a schema on a GenAI without llguidance (Windows) fails with `TextError::JsonSchemaUnsupported`, never an unconstrained answer; the SmolLM2 contract model comes from `resources/models/llm.lock` through `fetch-models.sh --llm` only, never the default fetch packaging calls, so no installer carries it.

The suite as every Linux lane runs it (`KATAGLYPHIS_REQUIRE_GPU=1`, `KATAGLYPHIS_REQUIRE_MODEL=1`, `KATAGLYPHIS_REQUIRE_GST=1`, `KATAGLYPHIS_REQUIRE_GENAI=1`), where the suites live, and the lint commands: [`docs/testing.md`](docs/testing.md). **Run the lint gates before pushing**: the fmt/clippy pair there is CI's `fmt-clippy` step (§ *`cargo_fmt_clippy.sh`*), and the shell/workflow/secret/config gates are `bash scripts/linux/run-lint-gates.sh` ([Continuous integration](#continuous-integration)).

**A hook runs them for you.** Once per clone: `git config core.hooksPath
third_party/ANTfrastructure/shared/linux/templates/git-hooks`. A commit then checks what is staged (large
files, secrets) in seconds; a push runs `scripts/linux/run-lint-gates.sh` and then
`scripts/linux/pre-push-checks.sh`, the script suites `lint-gates.yml` runs from the same file, two to three
minutes on a Pi 5. The gates print one warning line in a clone without the setting. Stage files by name, never
`git add -A`: that is how a core file reached a commit once.

**Benchmarks are Criterion's, run by `cargo bench`**. `crates/observant/benches/frame_stages.rs`
times what a frame pays before the model (I420 to RGBA, the privacy blocks, a box, the snapshot JPEG, the
motion thumbnail) and `crates/inference/benches/detector.rs` a whole detection on the real model at
ObservANT's two threads. The Linux lanes' *Run benchmarks* step (`ci-container-steps.sh bench`) fetches the
model and runs only the `[[bench]]` targets (`cargo bench --workspace --locked --bench '*'`; `--benches` also
ran every library's unit tests) in the *Benchmarks* job, on its own runner with its own sccache key
(`<arch>-bench`): the bench profile builds every dependency with `panic=unwind`, unlike the release
build's `abort`, and the build job's 1.5 GB cache always evicted those entries. The hub's
`bench-history` action then puts the table in the run's summary, each mean against the **median** of the
default branch's last 20 runs on that arch, read from the artifacts of earlier runs. **bench-history keys:**
`criterion-x64` and `criterion-arm64` (artifacts `bench-history-criterion-<arch>`, kept 90 days). More than
25 % slower warns; a gated mean (the detector, I420 to RGBA, the privacy blocks, the snapshot JPEG, the motion
thumbnail, all from 100 µs) that doubles fails the lane once 3 earlier runs exist, and such a run is stored as
failed, never as a yardstick. `draw_box` (1 µs) only warns. Locally:
`cargo bench --workspace --bench '*'`, where Criterion compares against your last local run. Benches need a library: `kataglyphis_observant`
is one since that day, `main.rs` only wiring its modules into the service.

**The detector meets its real model in every lane**. `crates/inference/tests/real_model.rs`
runs DEIM-D-FINE-N on `tests/fixtures/two_cats.jpg` and checks that both cats are found, where they
are, and taken for no dog or person. Without the model it skips; the `test` and `coverage` steps of
`scripts/linux/ci-container-steps.sh` fetch it first (`scripts/linux/fetch-models.sh`) and set
`KATAGLYPHIS_REQUIRE_MODEL=1`, so there a missing model fails, as `KATAGLYPHIS_REQUIRE_GPU` does
for the golden tests. On Windows `Invoke-DebugTests.ps1`, `Test-RustAll.ps1` and `Build-Windows.ps1`'s
*Unit Tests* fetch it (`Get-Models.ps1`) and require it, and `Stage-CrossTests.ps1` stages it for
the arm64 device ([Continuous integration](#continuous-integration)).

### Coverage and mutation testing

The `coverage` step measures every crate, and `check-crate-coverage.py` holds each to its floor.
What it needs, and the `cargo mutants` runs per file:
[`docs/testing.md`](docs/testing.md#coverage-and-mutation-testing).

### The cat cam service

`kataglyphis_observant` is one process doing everything an ObservANT board needs. How to run it, and
what its camera, capture, stream and web parts must keep doing:
[`docs/observant.md`](docs/observant.md#the-cat-cam-service); the Raspberry Pi 5 runner:
[§ The cat producer](docs/observant.md#the-cat-producer-on-a-raspberry-pi-5).

### Build & test in the Stevedore Windows container

The driver is `scripts\windows\container\Invoke-StevedoreBuild.ps1`. **No image reference is written
in this repository**, this file included; `verify_ci_image_refs.py` check D fails a copy. Host-side
mechanics are the hub's, so never copy their commands back here:
[`docs/windows.md`](docs/windows.md#build--test-in-the-stevedore-windows-container).

### Continuous integration

Linux x64 and the gates run on every push; the other build lanes wait for a commit message's token
(`[ci windows]`, `[ci arm64]`, `[ci linux]`, `[ci riscv64]`, `[ci x64]`, `[ci all]`), so a change to
code only those lanes build asks for its lane. Lane by lane: [`docs/ci.md`](docs/ci.md).

| Workflow | What it is |
| --- | --- |
| `linux-x64.yml`, `linux-arm64.yml` | Build, test, package and benchmark through `reusable-linux.yml` |
| `linux-riscv64.yml` | Cross-built on amd64, tested under QEMU; the GPU suites weekly |
| `windows-x64.yml`, `windows-arm64-cross.yml` | Thin callers of the hub's `container-ci-windows.yml` |
| `on-request.yml` | Reads the commit message's `[ci ...]` tokens and starts those lanes |
| `release.yml` | Every push to `main`: all five lanes, then `v<version>` |
| `retry-disk-refusal.yml` | Re-runs one job the hub's `prepare-linux-ci-host` refused for runner disk; never `On request`, `release.yml` or a GPU candidate lane |
| `docs.yml` | The rustdoc site and the `docs/` guides |
| `lint-gates.yml` | The hub's lint gates, PowerShell lint, script suites and generated-artifact checks; `bash scripts/linux/run-lint-gates.sh` is its local twin, and `scripts/linux/pre-push-checks.sh` its script suites |
| `powershell-quality.yml` | PowerShell complexity, strict on every push and PR |
| `python-static.yml` | Strict Ruff and ty |
| `python-refactor.yml` | On-demand refactor preview; uploads a patch and publishes nothing |
| `code-quality-report.yml` | `quality-report.sh` on every push and PR; uploads `code-quality-measurements` |
| `security-audit.yml` | The hub's lockfile audit, below |
| `ort-license-mutations.yml` | The package-license and ORT mutation recipes |
| `renovate-entrypoint.yml` | Checks the Renovate wrapper and config; runs no upgrade |
| `submodule-pins.yml` | The hub's pin suite when a gitlink moves |
| `linux-x64-amd-gpu.yml` | Dispatch only: an unverified AMD GPU package candidate |
| `linux-nvidia-gpu.yml`, `linux-nvidia-gpu-build.yml` | Dispatch only: unverified NVIDIA candidates and their reusable build |
| `windows-gpu-candidates.yml` | Dispatch only: unverified Windows GPU candidates |

**Security audit lane.** `security-audit.yml` calls the hub's reusable `security-audit.yml` on push,
pull request, Mondays and dispatch over `Cargo.lock`. Push and PR fail on a new HIGH or CRITICAL
advisory; the weekly run and dispatch report every finding.

**No build workflow names an image**: `verify_ci_image_refs.py` check D fails a reference retyped into
a workflow, a script or a comment. **Nothing generated may be tracked**, and a PowerShell unit over 15
cyclomatic, 5 nesting or 80 function lines fails the job: split it, never raise a limit.

**Lint the workflows locally** with the submodule's pinned actionlint. The trailing `.` is
load-bearing: without it the script lints the hub's workflows and reports green either way.

```bash
bash third_party/ANTfrastructure/linux/scripts/lint-workflows.sh .
```

### Dependency upgrades

Renovate, run as a **local CLI** (`bash scripts/linux/renovate-local.sh`); its commands,
what `--apply` writes, WSL and the token: [`docs/updates.md`](docs/updates.md). Every lane
above runs its gates, drivers and modules out of the `third_party/ANTfrastructure` pin
(§ 3), so that gitlink drifting is a silent change to every gate, and nothing watched it
before this wrapper existed. CI verifies the wrapper and config, without running upgrades;
the Renovate GitHub App remains disabled.

**A run reports. `--apply` only when the owner asked for it in that turn** (owner
directive 2026-09-11, a family rule the hub's `AGENTS.md` keeps). The family rationale:
[`third_party/ANTfrastructure/docs/dependency-updates.md`](third_party/ANTfrastructure/docs/dependency-updates.md).

## 6. Docs owned by this repo

**`run-lint-gates.sh --agents-drift` holds this file to the tree**: section 1 names every crate and
`src` module, section 5 every workflow and binary, `docs.yml` every page in `docs/`, and `BACKLOG.md`
every open `LAI-O` ID. A finding is fixed by naming the thing, never by an exception.

**Write docs like a developer: short and precise** (owner rule). Say what a thing does,
how to run it and what breaks; no filler, no marketing, no repetition.

`AGENTS.md` (this file), `README.md`, `BACKLOG.md`, `CHANGELOG.md`, the guides in
`docs/`, `crates/webgpu_renderer/README.md`, the renderer's design documents in
`crates/webgpu_renderer/docs/`, and the `PATCHED.md` of each vendored crate under
`third_party/` (its provenance and removal steps). Rustdoc is published to
<https://rust.jonasheinle.de> by `docs.yml` on every push to `develop` or `main`. Update
the docs in the same change as the behaviour they describe.

**The README is a short overview** ([the family's README rule](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/repository-family.md#the-readme-rule)).
Everything else is a guide in `docs/`, one row each in the README's *Documentation*
table: [`getting-started.md`](docs/getting-started.md), [`crates.md`](docs/crates.md),
[`testing.md`](docs/testing.md), [`running.md`](docs/running.md),
[`observant.md`](docs/observant.md), [`windows.md`](docs/windows.md),
[`packaging.md`](docs/packaging.md), [`text.md`](docs/text.md), [`ci.md`](docs/ci.md) and
[`updates.md`](docs/updates.md). A new guide
gets a row there too, and a line in `docs.yml`'s `guides`. **The README links with absolute GitHub URLs**, because it is
also the `oxidant` crate's rustdoc front page (`#![doc = include_str!("../README.md")]`
in `src/lib.rs`), where a relative link resolves under rust.jonasheinle.de and is a 404.
For the same reason every code block in it carries a language (`bash`, `pwsh`): an
untagged one is compiled as a doc test. The guides in `docs/` keep their relative links:
the site renders each one beside `oxidant/index.html` and points a link into the repository
at GitHub ([`getting-started.md` § API docs](docs/getting-started.md#api-docs)).

### Large tracked files and models

Only the README's hero image is tracked large; models are data, named with their SHA-256 in
`resources/models/models.lock` and fetched by `scripts/linux/fetch-models.sh`. The Rust crate compiles
the file name from the lock, and the class names are the hub's synced `coco-classes.txt`. What may join
them, and where the renderer's design documents live:
[`docs/crates.md`](docs/crates.md#large-tracked-files).

### Conventions

- Version pins/single sources of truth follow the ANTfrastructure ecosystem; don't duplicate what the submodule documents — link to it.
- Never commit build outputs: `/target`, root `/debug`, `/profile`, `/release` are gitignored, and so is `logs/`.
