<div align="center">
  <a href="https://jonasheinle.de">
    <img src="images/logo.png" alt="logo" width="200" />
  </a>

  <h1>OxidANT</h1>
 
  <h4>The Kataglyphis family's Rust crates (core, telemetry, inference, media, gui, cli, webgpu_renderer, cat_webrtc) and the Rust best practices they follow. Part of the <a href="https://github.com/Kataglyphis/ANTfrastructure">Kataglyphis Ecosystem</a> for robust code sharing and rapid development.</h4>
</div>

<div align="center">
  <a href="https://jonasheinle.de">
    <img src="images/Rust.gif" alt="Rust" width="400" />
  </a>
</div>

[![Linux x64 · build + test](https://github.com/Kataglyphis/OxidANT/actions/workflows/linux-x64.yml/badge.svg)](https://github.com/Kataglyphis/OxidANT/actions/workflows/linux-x64.yml)
[![Linux arm64 · build + test](https://github.com/Kataglyphis/OxidANT/actions/workflows/linux-arm64.yml/badge.svg)](https://github.com/Kataglyphis/OxidANT/actions/workflows/linux-arm64.yml)
[![Windows x64 · build + test](https://github.com/Kataglyphis/OxidANT/actions/workflows/windows-x64.yml/badge.svg)](https://github.com/Kataglyphis/OxidANT/actions/workflows/windows-x64.yml)
[![Windows arm64 · cross build + run](https://github.com/Kataglyphis/OxidANT/actions/workflows/windows-arm64-cross.yml/badge.svg)](https://github.com/Kataglyphis/OxidANT/actions/workflows/windows-arm64-cross.yml)
[![CodeQL](https://github.com/Kataglyphis/OxidANT/actions/workflows/github-code-scanning/codeql/badge.svg)](https://github.com/Kataglyphis/OxidANT/actions/workflows/github-code-scanning/codeql)

For **__official docs__** follow this [link](https://rust.jonasheinle.de).

> **Every platform lane runs on every push and PR** to `main`/`develop` — Linux x64, Linux arm64, Windows x64 and Windows arm64 (cross-built, then run on a real arm64 runner), one workflow each, so each badge above reports a real run. No commit-message marker is needed. The one opt-in job left is the Linux x64 feature check (`[build-features]` in the HEAD commit message, or a manual run). See [AGENTS.md](AGENTS.md#continuous-integration).

<!-- [![Linux build](https://github.com/Kataglyphis/GraphicsEngineVulkan/actions/workflows/Linux.yml/badge.svg)](https://github.com/Kataglyphis/GraphicsEngineVulkan/actions/workflows/Linux.yml)
[![Windows build](https://github.com/Kataglyphis/GraphicsEngineVulkan/actions/workflows/Windows.yml/badge.svg)](https://github.com/Kataglyphis/GraphicsEngineVulkan/actions/workflows/Windows.yml)
-->
[![TopLang](https://img.shields.io/github/languages/top/Kataglyphis/OxidANT)]() 
[![Donate](https://img.shields.io/badge/Donate-PayPal-green.svg)](https://www.paypal.com/paypalme/JonasHeinle)
[![Twitter](https://img.shields.io/twitter/follow/Cataglyphis_?style=social)](https://twitter.com/Cataglyphis_)
 
## Table of Contents

- [About The Project](#about-the-project)
  - [The crates](#the-crates)
  - [Dependencies](#dependencies)
  - [Useful tools](#useful-tools)
- [Getting Started](#getting-started)
- [Tests](#tests)
- [Run](#run)
- [Analysis](#analysis)
- [Cameras](#cameras)
- [Docs](#docs)
- [Updates](#updates)
  - [Dependency upgrades: Renovate as a local CLI](#dependency-upgrades-renovate-as-a-local-cli)
  - [Installed cargo binaries](#installed-cargo-binaries)
- [Contributing](#contributing)
- [License](#license)
- [Contact](#contact)
- [Literature](#literature)

## About The Project

OxidANT is the Kataglyphis family's **Rust workspace**: the crates two other
repositories build as a submodule, plus the Rust practices, gates and packaging
they all inherit. It started as a project template and that scaffolding is still
here — CI lanes, MSIX/MSI packaging, Renovate, lint gates, a feature matrix — but
the crates are real code with real consumers rather than placeholders.

The largest of them is **`crates/webgpu_renderer`**, a WebGPU (wgpu) glTF renderer
that runs natively (Vulkan/DX12/Metal) and in the browser (wasm32 + WebGPU): PBR
with IBL, cascaded shadow maps, SSAO, bloom, GPU skinning, animations, LOD, hot
shader reload, and headless golden tests. See
[`crates/webgpu_renderer/README.md`](crates/webgpu_renderer/README.md) for the
demos and the SPIR-V/GLSL shader-export pipeline it shares with the C++ Vulkan
engine in BeschleunigerBallett.

Containers, PowerShell and CI plumbing are **not** duplicated here. They belong to
[Kataglyphis ANTfrastructure](https://github.com/Kataglyphis/ANTfrastructure), the
submodule under `third_party/` that every repository in the family shares — see
[AGENTS.md](AGENTS.md) before writing a helper.

### The crates

One Cargo workspace. The root `Cargo.toml` is both the workspace and the root
package `oxidant` — a `cdylib`/`staticlib`/`rlib` library whose consumers are
other repositories, so its `[lib] name` is not free to change.

| Crate | Package | What it is |
| --- | --- | --- |
| `crates/core` | `kataglyphis_core` | Config, detection types, logging |
| `crates/telemetry` | `kataglyphis_telemetry` | CPU/GPU/RAM resource monitoring |
| `crates/inference` | `kataglyphis_inference` | ONNX backends, feature-gated: `onnx_tract`, `onnxruntime`, `onnxruntime_directml`, `onnxruntime_cuda` |
| `crates/media` | `kataglyphis_media` | GStreamer capture, feature-gated (`gstreamer`) |
| `crates/gui` | `kataglyphis_gui` | Feature-gated GUI: `gui_windows`, `gui_linux`, `gui_wgpu`, `gui_unix` |
| `crates/webgpu_renderer` | `kataglyphis_webgpu_renderer` | wgpu glTF renderer, native and wasm32/WebGPU: PBR+IBL, cascaded shadows, SSAO, bloom, skinning, animations, LOD, headless golden tests |
| `crates/cat_webrtc` | `kataglyphis_cat_webrtc` | Cat-detection WebRTC producer; consumed by OmniAccelerANT's Stream page |
| `crates/cli` | `kataglyphis_cli` | The CLI binary (`read` / `stats` / `gui`) |
| `src/` | `oxidant` | The root package: the flutter_rust_bridge surface for OmniAccelerANT, plus the feature-gated `burn-demos` bin |

**Default features are empty.** GUI, ONNX, GStreamer and the burn demos only
compile with an explicit `--features` (see [Run](#run)), and each one needs system
libraries — [AGENTS.md](AGENTS.md) has the feature/dependency table.

Two repositories build this one as a submodule, so a rename has to be carried into
both: [OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT) (the root
package through flutter_rust_bridge, and `crates/cat_webrtc`) and
[BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett)
(`crates/webgpu_renderer` and `crates/gui` through Corrosion).

### Dependencies

Crate versions live in `Cargo.toml`/`Cargo.lock`; the `third_party/ANTfrastructure`
gitlink is a dependency too, and the one that silently changes every gate.

For the newest versions your current constraints already allow (`Cargo.lock` only,
no manifest edit):

```bash
cargo update
```

To move the manifests as well:

```bash
cargo install cargo-edit
cargo upgrade --dry-run --verbose
cargo upgrade --incompatible
```

To see what is behind *before* moving anything — crates **and** the gitlink,
which is decided by Renovate rather than by a version bound — see
[Dependency upgrades](#dependency-upgrades-renovate-as-a-local-cli).

One lockfile entry is pinned by hand and a bare `cargo update` will undo it:
`zune-core` is held at 0.5.1 because 0.5.2 breaks `zune-jpeg` 0.5.15, and it only
shows up in a release build. [AGENTS.md](AGENTS.md) has the detail and the
one-line fix.

### Useful tools

* [cargo-outdated](https://github.com/kbknapp/cargo-outdated)
<!-- * [cppcheck](https://cppcheck.sourceforge.io/) -->

<!-- GETTING STARTED -->
## Getting Started

You need a Rust toolchain and the submodule. The toolchain version the CI images
and every gate use is `RUST_VERSION` in
[`third_party/ANTfrastructure/linux/scripts/01-core/versions.env`](third_party/ANTfrastructure/linux/scripts/01-core/versions.env) — read it there rather than
pinning a number here, which is how the last one went stale. Nothing else is
needed for a default-feature build.

```bash
git clone --recurse-submodules https://github.com/Kataglyphis/OxidANT.git
cd OxidANT
cargo build --workspace --locked
```

An existing clone without the submodule: `git submodule update --init --recursive`.

## Tests

Run the complete suite (unit + integration + proptest fuzz + doc tests) at the debug profile:

```bash
cargo test --workspace --locked
```

CI additionally gates on formatting and lints, both as hard failures. Run them before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The `crates/webgpu_renderer` headless golden tests need a GPU adapter and **silently skip without one**. Set `KATAGLYPHIS_REQUIRE_GPU=1` to turn a missing adapter into a failure, so a green run actually means they rendered:

```bash
KATAGLYPHIS_REQUIRE_GPU=1 cargo test --workspace --locked
```

The suites live in:

- Unit tests inside the workspace crates (currently `kataglyphis_telemetry`).
- Integration tests: `tests/integration.rs`.
- Fuzz (property-based) tests: `tests/fuzz_test.rs` via [proptest](https://proptest-rs.github.io/proptest/) (256 random inputs per case by default). There is no separate `cargo-fuzz`/libFuzzer setup.

Latest verified run (2026-08-07, Stevedore Windows container): the 8 tests that predate `crates/webgpu_renderer` pass — 3 integration, 1 proptest fuzz case, 4 telemetry unit.

**`kataglyphis_webgpu_renderer` is now excluded from the container run on purpose.** Any of its test binaries links wgpu, and wgpu's `gles` backend makes the executable import `opengl32.dll` at load time; Windows Server Core does not ship that DLL, so the process dies with `0xc0000135` (`STATUS_DLL_NOT_FOUND`) before `main`. The loader resolves that import, so no runtime flag avoids it — and letting it run turned the entire `cargo test --workspace` into a crash with no results. The `gles` feature is kept deliberately: it is the OpenGL fallback for machines without Vulkan/DX12. Run those tests on a desktop Windows host, where the DLL exists:

```pwsh
cargo test -p kataglyphis_webgpu_renderer --locked
```

CI does exactly that since 2026-09-24. In `windows-x64.yml` the container half
(`Invoke-WindowsLane.ps1`, which starts with `Invoke-DebugTests.ps1`) **builds** the
renderer's lib tests and records the executable in `target\host-tests\`. The lane's
`host-command` then runs `Invoke-HostTests.ps1` on the runner, which is a desktop
Windows Server with `opengl32.dll`. It fails if the list or the binary is missing, so
it can never report green over nothing.

Not a regression either way — the old "8 passed" figure was recorded a day before that crate existed. See [AGENTS.md](AGENTS.md) for the full analysis.

<!-- ROADMAP -->
## Run
```bash
cargo run -- read --path ../README.md
```

### Windows: GStreamer + ONNX Overlay (WGPU)

Build + Run (CPU via tract):

```bash
cargo run --bin kataglyphis_cli --features gui_windows,onnx_tract -- gui --backend dx12
```

Every `onnxruntime*` feature loads ONNX Runtime at run time and downloads
nothing at build time. The only ORT it may load is the family's chain build
(owner rule 2026-09-23): inside the Windows image that is
`$env:ONNX_ROOT\bin\onnxruntime.dll`, found automatically; anywhere else, stage
that file (plus `DirectML.dll` and `onnxruntime_providers_shared.dll`) next to
the exe or point `ORT_DYLIB_PATH` at it. There is no fallback to the
`onnxruntime.dll` Windows ships in System32, and any file that is not the chain
build (a pip wheel's, a GitHub release's) is refused at load time. The release
zip, MSIX and MSI carry the chain copy already.

Build + Run (ONNX Runtime + DirectML):

```bash
cargo run --bin kataglyphis_cli --features gui_windows,onnxruntime_directml -- gui --backend dx12
```

Build + Run (ONNX Runtime + CUDA, NVIDIA):

```bash
# PowerShell
$env:KATAGLYPHIS_ORT_DEVICE="cuda"
cargo run --bin kataglyphis_cli --features gui_windows,onnxruntime_cuda -- gui --backend dx12

# CMD
set KATAGLYPHIS_ORT_DEVICE=cuda
cargo run --bin kataglyphis_cli --features gui_windows,onnxruntime_cuda -- gui --backend dx12
```

Optional environment variables:

- `KATAGLYPHIS_ONNX_MODEL` – path to the ONNX model (default: `models/yolov10m.onnx`)
- `KATAGLYPHIS_ONNX_BACKEND` – `tract` or `ort` (default: automatic)
- `KATAGLYPHIS_ORT_DEVICE` – `cpu` | `auto` | `cuda` (default: `cpu`)
- `KATAGLYPHIS_PREPROCESS` – `letterbox` | `stretch` (default: `stretch`)
- `KATAGLYPHIS_SWAP_XY` – set to `1` if the model output swaps X and Y (default: `0`)
- `KATAGLYPHIS_SCORE_THRESHOLD` – detection score threshold (default: `0.5`)
- `KATAGLYPHIS_INFER_EVERY_MS` – inference interval in ms (default: `100`, `0` = every frame)

CUDA notes:

- Needs the NVIDIA driver plus the CUDA/cuDNN runtime on the machine, and a
  chain-built ONNX Runtime with the CUDA provider beside `onnxruntime.dll`.
  Nothing is copied from a download cache any more.
- If CUDA initialisation fails, `KATAGLYPHIS_ORT_DEVICE=auto` falls back to CPU.

The overlay shows FPS, inference latency, CPU/RSS and a CPU history, and inference
can be toggled from it.

## Analysis
```bash
cargo +nightly check --manifest-path Cargo.toml --target wasm32-unknown-unknown -Z build-std=std,panic_abort
```

### Resource usage logging (CPU/GPU/RAM)

```bash
cargo run --features gui_windows,onnxruntime_directml -- --resource-log --resource-log-interval-ms 1000 --resource-log-gpu=true gui
```

Optional, also write it to a file:

```bash
cargo run --features gui_windows,onnxruntime_directml -- --resource-log --resource-log-file .\resource.log gui
```

### Burn / PyTorch-replacement demos

A separate binary, behind the `burn_demos` feature.

```bash
cargo run --features burn_demos --bin burn-demos -- --help
```

Examples:

```bash
cargo run --features burn_demos --bin burn-demos -- tensor-demo

cargo run --features burn_demos --bin burn-demos -- linear-regression --epochs 50 --steps-per-epoch 50 --lr 0.02 --batch-size 256

cargo run --features burn_demos --bin burn-demos -- xor --epochs 2000 --lr 0.05

cargo run --features burn_demos --bin burn-demos -- two-moons --epochs 200 --steps-per-epoch 50 --lr 0.01 --batch-size 256

# ONNX Runtime YOLOv10m Demo (Default model: models/yolov10m.onnx)
cargo run --features burn_demos --bin burn-demos -- onnx-yolov10 --runs 1 --print-topk 3
```

### Windows
```bash
cargo run --features gui_windows -- gui --backend dx12

# Vulkan backend
cargo run --features gui_windows -- gui --backend vulkan

# Auto-select (wgpu PRIMARY)
cargo run --features gui_windows -- gui --backend primary
```

### Windows: build & test in the Stevedore container

The workspace builds and tests inside the [Kataglyphis ANTfrastructure](https://github.com/Kataglyphis/ANTfrastructure) Windows developer image using [Stevedore](https://github.com/slonopotamus/stevedore)'s `docker.exe`. The image reference is not written down anywhere in this repository — including here — because ANTfrastructure's `linux/scripts/01-core/versions.env` owns it; the driver asks `Get-CiImageReference -Windows` for it and `-Image` overrides for a one-off. To see the current value: `pwsh -c "Import-Module third_party/ANTfrastructure/windows/scripts/modules/WindowsContainerImage.Common.psm1; Get-CiImageReference -Windows"`.

> **ANTfrastructure is the ground truth for container and PowerShell functionality.** The scripts here are thin drivers: `docker.exe` discovery, isolation flags, container teardown, SDK-tool lookup, MSIX manifest expansion, config access and build-step logging all come from its modules under `windows/scripts/modules/`. Before adding a helper to `scripts/windows/`, check whether ANTfrastructure already has it — several that were written locally turned out to exist there in a better form. Everything is `pwsh` (PowerShell 7+); nothing here runs under Windows PowerShell 5.1.

The driver **bind-mounts this repository directly into the container** (as `C:\ws-mnt`) — no copy, so artifacts land straight in your tree and `third_party/` is available inside. It builds all three profiles (`dev`/debug, `profile` = release + debuginfo, `release` = fat LTO) and optionally the full debug test suite:

```pwsh
# build debug + profile + release in the container
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1

# build AND run cargo test --workspace (unit + integration + proptest fuzz + doc)
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1 -Test

# only if your host refuses the mount (see below)
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\container\Invoke-StevedoreBuild.ps1 -StageSources
```

> **Dev Drive (ReFS) is not a blocker — reading through a bind mount works.** What does not work is create-then-rename through it (`bindFlt` rejects `copySync`/`renameSync` with errno 3), which is precisely what cargo does. The driver keeps every build write container-local (`CARGO_TARGET_DIR=C:\ct`, `CARGO_HOME=C:\ch`), so only a plain artifact copy crosses the mount. If a host really does refuse it, `docker run` fails at once with *"Der Dateisystem-Minifilter kann nicht an das Entwicklervolume angefügt werden"*; fix it permanently with one elevated `fsutil devdrv setFiltersAllowed /volume D: "bindFlt,wcifs"` and a remount, or use `-StageSources` meanwhile. Note the two parts people get wrong: `/volume` is required, and the filter list is ONE quoted argument -- see third_party/ANTfrastructure/docs/windows-container-build-performance.md for the owning explanation. `fsutil devdrv query` needs elevation itself, so a failing query tells you nothing — just try the mount.

Artifacts land in `target\container\{debug,profile,release}` and are mirrored to the (gitignored) repo-root `debug\`, `profile\`, `release\` folders; each contains the CLI exe, cdylib (`.dll` + import lib), staticlib (`.lib`) and pdb. Latest verified run (2026-08-07, rustc 1.97.1): all three profiles built (debug 1m35s, profile 1m32s, release 1m12s), written straight into the repo through the mount, and the binaries run on the host, e.g.:

```pwsh
.\release\kataglyphis_cli.exe stats --path .\README.md
```

Host caveats the driver handles automatically. **ANTfrastructure is the authority on all of this** — these are pointers, not a second copy:

- `--isolation process` for the full host CPU count (Hyper-V isolation caps at 2), via `Get-ContainerIsolationArgs`.
- All cargo writes stay container-local (`CARGO_TARGET_DIR=C:\ct`, `CARGO_HOME=C:\ch`); only a plain artifact copy crosses the mount, because `bindFlt` rejects create-then-rename.
- A dropped docker CLI pipe does **not** mean the build died — the driver waits on the actual container state with upstream's `Wait-ContainerExit` (bounded at 60 min per phase), and tears containers down with `Remove-BuildContainerSafe`. A failed run's container is kept so `docker logs` still works.

| Topic | Read |
| --- | --- |
| Setting up a Windows host for Stevedore (services, `docker-users`, CNI nat conf) | [`docs/windows-host-setup.md`](third_party/ANTfrastructure/docs/windows-host-setup.md) |
| Windows container internals: wcifs/bindFlt, process isolation, layer-commit bug | [`docs/windows-builds.md`](third_party/ANTfrastructure/docs/windows-builds.md) |
| Running Linux containers on Windows (Rancher Desktop) | [`docs/rancher-desktop-linux-containers.md`](third_party/ANTfrastructure/docs/rancher-desktop-linux-containers.md) |
| Wiring a new project to all of it | [`docs/adopting-in-a-new-project.md`](third_party/ANTfrastructure/docs/adopting-in-a-new-project.md) |

### Linux containers locally (Rancher Desktop)

The Linux image is **always** the family CI one, in CI and locally — the same reference the workflows inherit, printed by `bash third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh` and owned by that submodule's `versions.env`. The command below asks for it rather than repeating it, so it cannot go stale on a tag bump. Rancher Desktop defaults to the **containerd** engine, so use `nerdctl`, not `docker` — and from Git Bash disable path mangling or the mount argument is destroyed. Full instructions: [`docs/rancher-desktop-linux-containers.md`](third_party/ANTfrastructure/docs/rancher-desktop-linux-containers.md).

```pwsh
$env:MSYS_NO_PATHCONV=1; $env:MSYS2_ARG_CONV_EXCL='*'
$image = bash third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh
rdctl shell nerdctl --namespace default run --rm --user root `
  -v kata-cargo-cache:/cargo-cache `
  -v /mnt/d/path/to/repo:/workspace -w /workspace `
  $image `
  bash -lc 'export CARGO_HOME=/cargo-cache; bash third_party/ANTfrastructure/linux/scripts/02-toolchain/rust/cargo_release.sh'
```

Two things that will bite on a Windows checkout, both verified 2026-08-07:

- **Shell scripts must be LF.** `.gitattributes` enforces it, but a checkout older than that rule keeps CRLF and bash dies on `set: pipefail\r: invalid option name`. One-time fix: `git ls-files -z '*.sh' | xargs -0 rm -f && git checkout -- .`
- `CARGO_HOME` in the image is root-owned, so point it at a writable path (a named volume keeps the registry across runs).

### Windows MSIX packaging

Needs the Windows SDK (`makeappx`, `signtool`) — located by ANTfrastructure's
`Resolve-WindowsSdkToolPath`, which honours VsDevCmd's `WindowsSdkVerBinPath` /
`WindowsSDKVersion` — and **PowerShell 7+ (`pwsh`)**: every script here carries
`#requires -Version 7.0` and will not start under 5.1. The pack itself is
ANTfrastructure's `Invoke-MsixPackage`; this repo supplies only the staging
directory (the release exe, the DLLs beside it, `resources/`) and the token map.

**The normal route is `Build-Windows.ps1`.** It packages MSIX itself, as its
**MSIX Packaging** step, taking every value from the `Msix` block of
`scripts/windows/Build-Windows.config.psd1` and letting an environment variable
override each one (`MSIX_PACKAGE_NAME`, `MSIX_DISPLAY_NAME`, …). `-SkipMsix`
turns it off. The package lands in `dist\windows-<x64|arm64>\msix\`, beside the
portable bundle and the MSI, and carries the exe's whole DLL closure.

```pwsh
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\Build-Windows.ps1
```

**The same route signs.** Its MSIX step signs with the first `*.pfx` at the
repository root (`*.pfx` is gitignored) and `MSIX_PFX_PASSWORD`, then verifies the
signature. With no `.pfx` there it warns and the package stays unsigned, which is
what CI builds. ANTfrastructure's `GenerateCertificateMSIX.ps1` makes a test
certificate. Its `-Publisher` must be the manifest's publisher, `CN=Kataglyphis` (the
`Msix` block's `Publisher`):

```pwsh
pwsh -File .\third_party\ANTfrastructure\windows\scripts\certificates\GenerateCertificateMSIX.ps1 `
  -Password '<TEST_CERT_PASSWORD>' -Publisher 'CN=Kataglyphis' -PfxPath .\Kataglyphis.OxidANT.testcert.pfx
$env:MSIX_PFX_PASSWORD = '<TEST_CERT_PASSWORD>'
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\Build-Windows.ps1
```

An existing PFX works the same way, and its subject must match the publisher too.
The package is `dist\windows-<arch>\msix\Kataglyphis.OxidANT_<VERSION>_<arch>.msix`,
with `VERSION.txt`'s version padded to four parts (`2.3.4` becomes `2.3.4.0`).
ANTfrastructure's standalone `New-MsixPackage.ps1`, which this section used to show,
cannot package this repo: it fills `__PACKAGE_NAME__`-style tokens, while
`packaging/msix/AppxManifest.template.xml` carries the `__MSIX_*__` tokens that
`Build-Windows.ps1` fills.

Installing a test-signed package needs an **elevated** PowerShell, because the
certificate has to be trusted machine-wide first:

```pwsh
$certPath = 'Kataglyphis.OxidANT.testcert.pfx'
$msixPath = 'dist\windows-x64\msix\Kataglyphis.OxidANT_2.3.4.0_x64.msix'
$pfxPw    = ConvertTo-SecureString '<TEST_CERT_PASSWORD>' -AsPlainText -Force

Import-PfxCertificate -FilePath $certPath -Password $pfxPw -CertStoreLocation 'Cert:\LocalMachine\Root'
Import-PfxCertificate -FilePath $certPath -Password $pfxPw -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople'

Add-AppxPackage -Path $msixPath
```

Both paths are repo-relative on purpose: the absolute `C:\GitHub\OmniAccelerANT\third_party\OxidANT\...` they used to carry was one
developer's checkout and was wrong for everyone else.

Check, launch, update, remove:

```pwsh
Get-AppxPackage -Name Kataglyphis.OxidANT | Select-Object Name, PackageFullName, Status

$pkg = Get-AppxPackage -Name Kataglyphis.OxidANT
Start-Process "shell:AppsFolder\$($pkg.PackageFamilyName)!App"

# update: raise VERSION.txt, rebuild and sign, then install it the same way
Add-AppxPackage -Path dist\windows-x64\msix\Kataglyphis.OxidANT_<NEW_VERSION>_x64.msix

Get-AppxPackage -Name Kataglyphis.OxidANT | Remove-AppxPackage
```

Troubleshooting:

- `0x800B0109` — the certificate chain is not trusted. Import the certificate into
  both `LocalMachine\Root` and `LocalMachine\TrustedPeople` as above (needs admin).
- `Import-PfxCertificate: Access denied` — the shell is not elevated.
- `Get-AppxLog -ActivityID <ACTIVITY_ID>` prints the detail behind the last deploy
  failure.

The certificate half of this belongs upstream and is partly there already:
[`third_party/ANTfrastructure/windows/scripts/certificates/README.md`](third_party/ANTfrastructure/windows/scripts/certificates/README.md).

**The package identity changed on 2026-09-05** from
`Kataglyphis.RustProjectTemplate` to `Kataglyphis.OxidANT`. Windows treats the two
as different applications, so an installation predating that date is not upgraded —
uninstall it first.

### Windows MSI packaging

Runs as the **MSI Packaging** step of `Build-Windows.ps1` (disable with `-SkipMsi`,
or `Msi.Enabled = $false` in `scripts/windows/Build-Windows.config.psd1`).

Output: `dist\windows-<x64|arm64>\msi\kataglyphis_cli-<VERSION>-<x64|arm64>.msi`

Built with **WiX Toolset v4** (`wix.exe build`), not `cargo-wix`: cargo-wix drives
WiX v3's `candle.exe`/`light.exe` even in its newest release (0.3.9), while the
container image ships WiX 4.0.6 as a single `wix.exe`. `wix/main.wxs` is therefore
in the v4 schema and gets version, binary path and licence path as preprocessor
variables. The `WixUI_FeatureTree` dialog set comes from `WixToolset.UI.wixext`,
already installed in the image.

### Linux
```bash
# WGPU (recommended)
cargo run --features gui_wgpu -- gui --backend vulkan

# GTK demo
cargo run --features gui_unix -- gui
```

## Docs
```bash
cargo doc --open
```
## Updates

### Dependency upgrades: Renovate as a local CLI

What is behind — the submodule gitlink and the workspace crates — is answered by
Renovate run locally, not by a bot. Run it from WSL; it bootstraps a pinned,
checksum-verified Node and Renovate on first use.

```bash
bash scripts/linux/renovate-local.sh                    # report (default: git-submodules)
bash scripts/linux/renovate-local.sh --managers cargo   # the workspace crates
bash scripts/linux/renovate-local.sh --apply --dry-run  # the plan
bash scripts/linux/renovate-local.sh --apply            # move the gitlink
```

The Renovate GitHub App is installed on no repo in this family and will not be,
so this wrapper is the only thing that ever reads `.github/renovate.json`. It is
not a gate: no workflow runs it and it blocks no commit. `--apply` moves
**gitlinks only** — the crates stay a `cargo upgrade` job — and it stages and
commits nothing. Details in the script header and in
[`third_party/ANTfrastructure/docs/dependency-updates.md`](third_party/ANTfrastructure/docs/dependency-updates.md).

### Installed cargo binaries

How to update all installed packages:

1. Install updater:
  ```bash
  cargo install cargo-update
  ```
2. Now update all packages:
  ```bash
  cargo install-update -a
  ```

## Cameras

Raw `gst-launch-1.0` pipelines — listing a device's formats, bisecting a capture
problem below the application, Raspberry Pi CSI and V4L2 sources, MJPEG, hardware
encoders that under-declare their caps — are ANTfrastructure's, in
[`third_party/ANTfrastructure/docs/runtime-services.md`](third_party/ANTfrastructure/docs/runtime-services.md),
§ *Raw `gst-launch-1.0` pipelines (debugging below the app)*. They were copied
here as three bare commands with no explanation of when to reach for them.

<!-- CONTRIBUTING -->
## Contributing

Contributions are what make the open source community such an amazing place to be learn, inspire, and create. Any contributions you make are **greatly appreciated**.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request


<!-- LICENSE -->
## License

Distributed under the MIT License. See [LICENSE](LICENSE).

<!-- CONTACT -->
## Contact

Jonas Heinle - [@Cataglyphis_](https://twitter.com/Cataglyphis_) - jonasheinle@googlemail.com

Project Link: [https://github.com/Kataglyphis/OxidANT](https://github.com/Kataglyphis/OxidANT)


## Literature 

Some very helpful literature, tutorials, etc. 

<!-- CMake/C++
* [Cpp best practices](https://github.com/cpp-best-practices/cppbestpractices)

Vulkan
* [Udemy course by Ben Cook](https://www.udemy.com/share/102M903@JMHgpMsdMW336k2s5Ftz9FMx769wYAEQ7p6GMAPBsFuVUbWRgq7k2uY6qBCG6UWNPQ==/)
* [Vulkan Tutorial](https://vulkan-tutorial.com/)
* [Vulkan Raytracing Tutorial](https://developer.nvidia.com/rtx/raytracing/vkray)
* [Vulkan Tutorial; especially chapter about integrating imgui](https://frguthmann.github.io/posts/vulkan_imgui/)
* [NVidia Raytracing tutorial with Vulkan](https://nvpro-samples.github.io/vk_raytracing_tutorial_KHR/)
* [Blog from Sascha Willems](https://www.saschawillems.de/)

Physically Based Shading
* [Advanced Global Illumination by Dutre, Bala, Bekaert](https://www.oreilly.com/library/view/advanced-global-illumination/9781439864951/)
* [The Bible: PBR book](https://pbr-book.org/3ed-2018/Reflection_Models/Microfacet_Models)
* [Real shading in Unreal engine 4](https://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf)
* [Physically Based Shading at Disney](https://blog.selfshadow.com/publications/s2012-shading-course/burley/s2012_pbs_disney_brdf_notes_v3.pdf)
* [RealTimeRendering](https://www.realtimerendering.com/)
* [Understanding the Masking-Shadowing Function in Microfacet-Based BRDFs](https://hal.inria.fr/hal-01024289/)
* [Sampling the GGX Distribution of Visible Normals](https://pdfs.semanticscholar.org/63bc/928467d760605cdbf77a25bb7c3ad957e40e.pdf)

Path tracing
* [NVIDIA Path tracing Tutorial](https://github.com/nvpro-samples/vk_mini_path_tracer/blob/main/vk_mini_path_tracer/main.cpp) -->

<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->
