# AGENTS.md

Guidance for AI agents (and humans) working in OxidANT.

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
`tests/` hold, and the vendored `third_party/egui-winit-0.36.2`:
[`docs/crates.md`](docs/crates.md). The one git submodule, `third_party/ANTfrastructure`,
is **the ground truth for every container and PowerShell concern**; read § 2 before
writing any helper.

### Consumers

Repositories that build this one as a submodule. A rename or a `[lib] name` change goes
into each of them **in the same change**:

- [OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT): the root package
  through flutter_rust_bridge (Cargokit, podspecs, the committed `frb_generated.dart`
  loader stem), and `crates/observant` for its Stream page.
- [BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett): the root
  package through Corrosion, and `crates/webgpu_renderer` through plain cargo (its wasm32
  demo for the docs site, its test suite and the wasm size budget:
  `scripts/linux/{docs-build-web,run-cargo-tests,wasm-size-budget}.sh` there). The import
  is `Src/CMakeLists.txt:74-80`: `corrosion_import_crate(MANIFEST_PATH
  ../third_party/OxidANT/Cargo.toml CRATE_TYPES staticlib CRATES oxidant)`, guarded by
  `if(RUST_FEATURES)` (an ON/OFF option), then
  `corrosion_add_cxxbridge(oxidant_bridge ...)` over `src/native_only.rs`. It names the
  package `oxidant` and takes the **staticlib**, so `[package] name`, `[lib] name` and
  the `crate-type` list are all part of that repository's build. Since 2026-10-20 that
  staticlib links GStreamer on Linux and Windows (the webcam engine), so the link that
  takes it links GStreamer too.

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

## 3. Critical invariant: submodule pins

Builds are only supported against the **recorded submodule gitlink**, the commit CI
builds green; `git submodule update --checkout --recursive` restores it. If a drifted
submodule is what you want, move the gitlink **and** fix the fallout in the same change;
do not fork upstream behaviour locally.

**The `.gitmodules` url is `https://`, never `git@github.com:`**: a recursive checkout of
either of the [Consumers](#consumers) inherits it. Why, and how to change one:
[the hub's § *Submodule URLs are https, never `git@`*](third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#submodule-urls-are-https-never-git).

The one submodule, `third_party/ANTfrastructure`, supplies every gate: the
shellcheck/actionlint/gitleaks binaries the lint lane bootstraps, the cargo, packaging
and docs drivers, the Windows modules and the ORT census (G6). A drifted gitlink silently
changes every gate, and `git submodule status` marks it only with a `+`, easy to miss in
CI output. The images are the exception: the lanes take them from the hub's actions and
reusable workflows at `@develop`, which read `versions.env` at that ref, not at the pin
(009617e, 2026-09-25).

**Guarded by ANTfrastructure's repo-agnostic Pester suite**, run after any pin bump from
[`.github/workflows/submodule-pins.yml`](.github/workflows/submodule-pins.yml), since
2026-09-15 one `uses:` line onto the hub's reusable `submodule-pins.yml` plus the `on:`
filters that say when it runs. The suite,
`third_party/ANTfrastructure/shared/windows/tests/Submodule.Pins.Tests.ps1`, asserts for
**every** configured submodule that it is checked out, sits at its recorded commit, and
is pinned to a commit its remote still has, so a second submodule is covered the day it
lands. The Pester pin (`3.4.0`) and the `windows-2025` runner are the hub's choices, and
their forty lines of rationale live upstream once
(`third_party/ANTfrastructure/docs/windows-builds.md`). It stays a **separate workflow
from `lint-gates.yml`** because Pester 3.4.0 is a Windows PowerShell-era module never
released for PowerShell Core on Linux.

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

**The pdb collision is fixed; do not bring it back by renaming the bin.** Cargo warned that the root **lib** and the CLI **bin**, both then named `kataglyphis_rustprojecttemplate`, wrote the same `kataglyphis_rustprojecttemplate.pdb` (from the lib's `cdylib` crate type, not the rlib), and that this *"may become a hard error in the future"* ([rust-lang/cargo#6313](https://github.com/rust-lang/cargo/issues/6313)). A bin named `oxidant` today would collide on `oxidant.pdb`.

The **bin** was renamed, not the lib, on purpose: the C++ Vulkan engine imports the lib through Corrosion/cxxbridge (the generated `oxidant_bridge` target in the parent repo's CMake), so `[lib] name` decides DLL/LIB filenames another repository depends on. What moved with the bin: `Msix.Binary` and `Msi.OutputName` in `scripts/windows/Build-Windows.config.psd1`, `File Name=` in `wix/main.wxs`, `BINARY_FILE` in the Linux lane (`reusable-linux.yml` today) and `BINARY` in the Windows one (then `rust_windows2025.yml`), the `-Binary` default in `Invoke-AppProfiles.ps1`, and `--bin` in `scripts/linux/run-person-detection.sh`. Since 2026-09-25 the Windows lanes set no `BINARY`; the exe name sits in their `artifact-name`, `host-command` and `run-command` (`windows-x64.yml`, `windows-arm64-cross.yml`).

**`[lib] name` did change later, on 2026-09-05**, when the repository became OxidANT: `[package] name` and `[lib] name` are both `oxidant`, so the artefacts are `oxidant.dll` / `liboxidant.so` / `liboxidant.a`. That is the outside-this-repo break the paragraph above warns about, and it was safe only because every consumer moved in the same commit: OmniAccelerANT's Cargokit wiring, podspecs, `Get-WindowsBuildConfig.ps1` and the committed `frb_generated.dart` loader stem, plus BeschleunigerBallett's Corrosion import. Another rename means finding them again: [Consumers](#consumers).

**`BINARY` means different things per platform.** On **Windows** it is an optional environment override of `Msix.Binary`, the executable `Build-Windows.ps1` builds and packages (`kataglyphis_cli`). In the **Linux** workflow (`reusable-linux.yml`, which both Linux lanes call) it is `oxidant` and names every Linux package and the command each installs: `scripts/linux/package-linux.sh` puts `target/release/$BINARY_FILE` into the bundle `oxidant/` as `oxidant`, packs that directory as the tar, `/opt/oxidant/` behind `/usr/bin/oxidant` in the deb, the AppImage's payload, and `org.kataglyphis.oxidant`'s command in the flatpak. So `BINARY_FILE` is the cargo artefact, `BINARY` what a user invokes.

**The Linux packages carry their runtime closure** (2026-10-20), by the hub's rules ([`linux-build-basics.md` § *Linux packages that run outside the image*](third_party/ANTfrastructure/docs/linux-build-basics.md#linux-packages-that-run-outside-the-image)), since the CLI links GStreamer and loads ORT: the hub's `elf-closure.sh` puts the image's GStreamer, GLib, FFmpeg and GCC runtime into `lib/` with `$ORIGIN` RUNPATHs, the chain ORT goes in unpatched, `gstreamer-bundle.sh` copies the GUI pipeline's plugins into `lib/gstreamer-1.0`, `bundle-loader.sh --beside` leaves the image's glibc for an older host, and the model sits in `resources/models`. Before packing, `elf-closure.sh --check`, the G6 census, `gstreamer-bundle.sh check` and the bundle's own `--help`, `onnx-runtime` and `media-check` must pass; the `clean-host` job then installs the `.deb`, unpacks the tarball and starts the AppImage on a bare runner. `kataglyphis_media::ensure_gst_initialized` sets the four GStreamer variables that keep a package to its own plugins, each only when unset.

### Inside the Stevedore Windows container

Because cargo is what makes it bite:

- **The sources go in, and the artifacts come out, by tar-pipe** (since 2026-10-19).
  `Invoke-StevedoreBuild.ps1` is a spec for the hub's `Invoke-RepoContainerBuild.ps1`: it
  streams the tree into `C:\ws` of a reused, named container (`kata-rust-build`), without
  `.git`, `target`, `dist`, `logs` and the root mirrors, and streams
  `target\container\<profile>` and `logs` back. Nothing is bind-mounted, so a host's Dev
  Drive filters do not matter and the old `-StageSources` and `C:\ws-mnt` are gone.
- **cargo's trees stay outside the workspace**: `CARGO_TARGET_DIR=C:\ct`,
  `CARGO_HOME=C:\ch`. The runner prunes `C:\ws` before each run and the container keeps
  both, so the next run builds incrementally; `-FreshContainer` starts from nothing.
- **Container plumbing comes from the hub, never inline**: the runner resolves docker and
  the image, sets the isolation and memory, and with `-VerifyDirs` runs
  `Test-BuildArtifactsDelivered`, which fails when an exe built in the container did not
  reach the host (a green build is not proof of delivery). The in-container logs land in
  the workspace's `logs\` and come back on a failed run too. `-WhatIf` prints the
  in-container commands and starts nothing.
- **Everything here is `pwsh` (PowerShell 7+).** Every script under `scripts/windows/`
  carries `#requires -Version 7.0`, so a surviving "Windows PowerShell 5.1" comment is
  wrong: those scripts would not start under it. Keep `$ErrorActionPreference` at
  `Continue` in the in-container scripts and check `$LASTEXITCODE` manually anyway.
- **Still not adopted:** `Test-ContainerBindMount`, which the runner calls only under its
  `-UseBindMount`; the tar-pipe needs no probe.

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

### Never expand a manifest template with `-replace`

PowerShell treats the replacement side as a substitution template, so a value containing `$&` re-inserts the whole matched token. A description of ``Renderer $& x`` produced `Desc="Renderer __MSIX_DESCRIPTION__amp; x"`, the literal token, shipped into the manifest. `Expand-XmlTemplateTokens` uses an ordinal `[string].Replace` and escapes each value itself.

### Targets and their system dependencies

**No crate has a Cargo feature, and none may get one** (owner, 2026-10-20): every
dependency is unconditional or `[target.'cfg(...)'.dependencies]`, every gate in the code is
a target `cfg` or the root's `cfg(desktop)` (Linux, Windows, macOS, set by `build.rs`). A
`--features` on the command line fails: *"the package 'oxidant' does not contain these features"*.

| Target | What builds | Needs |
| --- | --- | --- |
| Linux x64/arm64 (`:latest`), riscv64 (cross, the image's sysroot), Windows x64/arm64 (`:winamd64`, `:winarm64`) | The whole workspace: tract plus the chain ORT with CUDA registered (DirectML on Windows), GStreamer capture, the webcam engine, the wgpu GUI, the ESP32 bridge, the burn demos | GStreamer's development files (source-built into `/opt/gstreamer` on `PKG_CONFIG_PATH`; never the distro's `libgstreamer*-dev`, which the image purges), the chain ORT at run time |
| `aarch64-linux-android` (OmniAccelerANT's Cargokit) | `oxidant` alone: the detector on tract; the webcam and ESP32 calls answer that the target has neither | The NDK: no GStreamer, serial port or chain ORT exists there |
| `wasm32-unknown-unknown` (OmniAccelerANT's web build) | `oxidant` alone, without the detector, whose model loads from a file the web cannot have; tract built (2026-10-20) but took the release wasm from 0.66 to 15.5 MB | The wasm target |

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
- **The GTK4 GUI is the one thing not built yet**: today's `:latest` has no usable GTK4
  development files (its `/opt/gstreamer` `gtk4.pc` names a `graphene-gobject-1.0` it lacks,
  2026-10-20). GTK 4 >= 4.10 (gtk4-rs 0.11, `v4_10`) comes with the next image, the first built
  after hub da25fa01 (CON129): amd64 against `/opt/gstreamer`'s GTK 4.14.5, arm64 and riscv64
  against Ubuntu's `libgtk-4-dev` 4.22.4, the riscv64 sysroot carrying it, and the consumer-image
  contract's row 9 (`gtk4-dev`) gating every arch. Once that image is published, `BACKLOG.md`'s
  change turns the GUI on; running it needs Xvfb (the image sets `GDK_BACKEND=x11`), and
  `GTK_A11Y=none` silences the accessibility-bus warning.
- **The camera GUI's wgpu has no GL backend** (`crates/gui/Cargo.toml`): its `GpuBackend`
  never picks GL, and the `opengl32.dll` import GL brings stops the CLI from loading in
  Windows Server Core ([docs/testing.md](docs/testing.md#on-windows-the-renderers-tests-run-outside-the-container)).

A plain Ubuntu box (e.g. the WSL recipe below) *does* need the distro packages, because nothing there provides the source-built stack. That difference is why "install the -dev package" is the wrong instinct when the image is involved.

### Known gaps

- **On arm64 the renderer tests run llvmpipe at 128-bit vectors** (since 2026-10-06). The image sets `LP_NATIVE_VECTOR_WIDTH=256` on every arch for Mesa's BVH sort (hub CON44), and at that width LLVM's AArch64 JIT killed a renderer test binary in about one arm64 run in five: `LLVM ERROR: Do not know how to split this operator's operand!` (SIGABRT; runs 37367760093, 37382902869, 37005853556) and a SIGSEGV in `forward_ambient` (run 37428962397). Nothing here uses ray tracing or subgroups, so `scripts/linux/ci-container-steps.sh` exports 128, arm64's native width, for every step on an aarch64 host.
- **The Linux lanes render the golden tests on a software Vulkan device, and must** (since 2026-09-29). The family image carries lavapipe (`vulkaninfo --summary`: deviceType CPU, `llvmpipe`) on amd64 and arm64 since ANTfrastructure CON19, and the `test` step of `scripts/linux/ci-container-steps.sh` exports `KATAGLYPHIS_REQUIRE_GPU=1`, so a green run means the ~40 headless render tests drew something. Before that no CI lane had an adapter, and every one of them reported as passed having drawn nothing. Both Windows lanes set it too since 2026-10-01 (`windows-x64.yml`'s `host-command`, `windows-arm64-cross.yml`'s `test-command`) and render on the runners' WARP. Only the riscv64 lane leaves it unset by default (§ *The riscv64 lane*). What the flag does to `GpuContext::headless_or_skip()`, the `ADAPTER:` line and the tests that fail rather than skip under it: [`docs/testing.md` § *The GPU tests must render*](docs/testing.md#the-gpu-tests-must-render). Running them for real (WSL + llvmpipe, 2026-08-07) surfaced a **pre-existing, deterministic rendering bug**, which the image's lavapipe reproduces byte-for-byte (987 pixels, 2026-09-29):

  ```
  a_non_uniform_instance_scale_shades_like_the_same_node_scale
  crates/webgpu_renderer/tests/skinned_bounds.rs
  node-scale and instance-scale must shade the same, 987 pixels differ  (threshold: 40)
  ```

  Confirmed pre-existing by re-running it against a pristine export of `4909bd7`: byte-identical 987. Everything else in the suite (~340 tests) passes. The failing path is the instanced normal transform: the generated `src/shaders/forward.wgsl` applies `instance_cofactor_0` to `worldNormal_0`, and the two shading paths disagree where they must agree.

  **Fix it upstream, not here.** The `.wgsl` files in `src/shaders/` are checked-in *generated artifacts*; there is no `.slang` file in this repo, and the code comments cite the C++ engine's `forward.slang` by line number (e.g. `cascades.rs` → `forward.slang:151`). Hand-editing the generated WGSL would desynchronise it from its source.

  **That one test is `#[ignore]`d, with the reason in the attribute**, the only scope cut on `KATAGLYPHIS_REQUIRE_GPU`: it fails on every adapter, software or not, so letting it red the lane would only have pushed the flag back out. `cargo test -p kataglyphis_webgpu_renderer --test skinned_bounds -- --ignored` still runs it; remove the attribute with the fix.

### Tests and scripts that read source text

- `crates/webgpu_renderer/tests/buffer_desc_single_definition.rs` pins the two `BufferDescriptor` outliers by
  line number (`occlusion.rs:358`, `histogram.rs:102`), so a comment edit above either line breaks it.
- `TEXTURE_2D_SHAPE_OK` must stay on the first line of its `wgpu::TextureDescriptor {` body
  (`tests/texture_desc_single_definition.rs`).
- `scripts/linux/check-ort-chain-only.sh --help` prints lines 2-10 of the file (`sed -n '2,10p'`).
- The two bootstrap copies (`scripts/linux/lib/antfrastructure.sh`, `scripts/windows/Resolve-BuildModule.ps1`)
  must equal the hub templates below their header (`sync-shared-config.sh --check`). Re-copy them after an
  upstream change; never edit them here.

### Verifying locally on a Windows box (no MSVC required)

Dev machines here often lack the MSVC "C++ build tools" workload. Without it **nothing links**, and Git Bash makes the failure baffling: `/usr/bin/link.exe` is coreutils' `link`, which shadows the MSVC linker on PATH and dies with `link: missing operand after '\377\376'` (it is handed rustc's UTF-16 response file). That is an environment fault, never a code fault.

The fastest real fix is to verify in WSL against the CI's own target platform:

```bash
# once, as root inside the distro
apt-get install -y build-essential pkg-config curl git libssl-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-base libgtk-4-dev
# optional: a software Vulkan adapter so the headless golden tests actually run
apt-get install -y mesa-vulkan-drivers vulkan-tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/ri.sh
# The toolchain is the fleet's, read from the submodule rather than typed here - it
# was pinned to a literal 1.97.1 until versions.env moved to 1.98.1 underneath it.
RUST_VERSION="$(. third_party/ANTfrastructure/linux/scripts/01-core/versions.env; echo "$RUST_VERSION")"
sh /tmp/ri.sh -y --profile minimal --default-toolchain "$RUST_VERSION" -c rustfmt -c clippy
```

Then, from the repo root, with `CARGO_TARGET_DIR` on a **Linux-native** path (never the 9p-mounted Windows tree, which is glacial and already holds MSVC artifacts):

```bash
export CARGO_TARGET_DIR=/root/kt
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
KATAGLYPHIS_REQUIRE_GPU=1 cargo test --workspace --locked
```

The flag is the CI lanes' ([`docs/testing.md` § *The whole suite*](docs/testing.md#the-whole-suite)): without it the golden-test suite "passes" having rendered nothing. The ORT tests need the chain ORT, which a WSL box lacks; the image has it.

## 5. Build, run, test

```bash
cargo build --workspace --locked                      # dev/debug
cargo build --workspace --locked --profile profile    # custom: release + debuginfo
cargo build --workspace --locked --release            # fat LTO, codegen-units 1, panic=abort, stripped
cargo test  --workspace --locked                      # unit + integration + proptest fuzz + doc tests
```

The suite as every Linux lane runs it (`KATAGLYPHIS_REQUIRE_GPU=1`, `KATAGLYPHIS_REQUIRE_MODEL=1`, `KATAGLYPHIS_REQUIRE_GST=1`), where the suites live, and the lint commands: [`docs/testing.md`](docs/testing.md). **Run the lint gates before pushing**: the fmt/clippy pair there is CI's `fmt-clippy` step (§ *`cargo_fmt_clippy.sh`*), and the shell/workflow/secret/config gates are `bash scripts/linux/run-lint-gates.sh` ([Continuous integration](#continuous-integration)).

**Benchmarks are Criterion's, run by `cargo bench`** (2026-10-14). `crates/observant/benches/frame_stages.rs`
times what a frame pays before the model (I420 to RGBA, the privacy blocks, a box, the snapshot JPEG, the
motion thumbnail) and `crates/inference/benches/detector.rs` a whole detection on the real model at
ObservANT's two threads. The Linux lanes' *Run benchmarks* step (`ci-container-steps.sh bench`) fetches the
model and runs every bench of the workspace; *Benchmark report* puts `scripts/linux/bench-report.py`'s table
in the run's summary, each mean against the **median** of the default branch's last 20 runs on that arch,
whose means the cache keeps in `.bench-history/` (2026-10-15; until then the run before, so a slowdown
creeping in a few per cent per commit never showed). More than 25 % slower warns; a mean from 100 µs that
doubles fails the lane once 3 earlier runs exist, and such a run never joins the history. Faster benches only
warn, since a shared runner's caches and clocks move them that much. Locally:
`cargo bench --workspace --benches`, where Criterion compares against your last local run. Benches need a library: `kataglyphis_observant`
is one since that day, `main.rs` only wiring its modules into the service.

**The detector meets its real model in the Linux lanes** (2026-10-14). `crates/inference/tests/real_model.rs`
runs DEIM-D-FINE-N on `tests/fixtures/two_cats.jpg` and checks that both cats are found, where they
are, and taken for no dog or person. Without the model it skips; the `test` and `coverage` steps of
`scripts/linux/ci-container-steps.sh` fetch it first (`scripts/linux/fetch-models.sh`) and set
`KATAGLYPHIS_REQUIRE_MODEL=1`, so there a missing model fails, as `KATAGLYPHIS_REQUIRE_GPU` does
for the golden tests.

### Coverage and mutation testing

- **Coverage measures the workspace** (since 2026-10-01). The hub's `cargo_coverage.sh` runs tarpaulin without `--workspace`, so it measured only the root package, which has no unit tests: `0.00% coverage, 0/754 lines covered` (run 36868485805). The `coverage` step passes `--workspace`, and sets `KATAGLYPHIS_REQUIRE_GPU=1` like the `test` step. It fails below 70 % of the workspace's lines (tarpaulin `--fail-under`; 72.9 % measured 2026-10-19, 71.0 % on 2026-10-20 once the GUI, which needs a window for most of its code, joined the figure); raise the floor as tests are added. Since 2026-10-15 it also grades each crate and `src/` against its own floor (`COVERAGE_FLOORS`, read by `scripts/linux/check-crate-coverage.py` from `cobertura.xml`), so the renderer's 90 % no longer carries telemetry's 8 %, and gives tarpaulin `--timeout 300` (its 60 s per test binary was too short on the Pi for an instrumented ObservANT).
- **Every module counts since 2026-10-20.** Tarpaulin leaves out every module behind `#[cfg(feature = …)]` (measured 2026-10-15, tarpaulin 0.37.5), so `crates/media` and `src/webcam_engine.rs` were absent from the report while their tests passed; with no features left they, the GUI and the burn demos are in it, each with a floor of its own.
- **A test binary must not see the distro's `libstdc++` first.** Tarpaulin puts every `rustc-link-search` path a build script prints on `LD_LIBRARY_PATH`; font-kit's fontconfig and freetype printed `/usr/lib/x86_64-linux-gnu`, its `libstdc++` (GCC 15) won over GCC 16.2's, and the chain ORT failed to `dlopen` there alone (2026-10-20), which `real_model.rs`'s backend check caught. The burn demos' plots use plotters' `ab_glyph` instead; a new system library linked through pkg-config brings the same trap back.
- **Mutation testing measures whether the tests notice a change** (2026-10-15). `cargo mutants -p kataglyphis_observant --file crates/observant/src/web.rs --file crates/observant/src/static_files.rs --jobs 1 -- --lib` (cargo-mutants 27.1.0, installed with `cargo install --locked cargo-mutants`) plants small bugs in ObservANT's code, one at a time; that command covers its network-facing code. Run it after changing the files below. Caught, of the mutants that compile:

  | Files | At first | After tests for each gap | The gaps | Left, and why |
  | --- | --- | --- | --- | --- |
  | `web.rs`, `static_files.rs` (163 mutants) | about 84 % | 142 of 145 | content types, the gzip cache, route resolution, peer CORS, the signalling proxy's leftover bytes, split and endless response heads, re-framing a refusal | 3 change nothing a client can see (HEAD and 304 bodies hyper drops anyway, a log line's throttle, `>` against `>=` at a 16 KiB limit) |
  | `motion.rs`, `presence.rs`, `status.rs`, the arrival logic (393 mutants) | about 92 % of the 379 that compile | 375 | exact box margins, the change and motion thresholds, an IoU of exactly 0.5, the visits file's fallback, the settings' limits, a stale frame at 21 s, the failure count | 4 only change a log line or a bound no test can see |
  | `camera.rs`, `config.rs`, `notify.rs` | about 90 % of 145 | 134 | a webcam by driver or bus, a format it can decode, rpicam-vid without a sensor, a configured `/dev/video` path, each config limit taken itself, a short webhook host | 11 read real camera devices, which no CI host has, or change a log line |
  | `clip.rs`, `frame.rs`, `stats.rs`, `output.rs`, `precompress.rs` | 376 of 412 | 400 | the privacy filter's margin, block mean and chroma planes, the box outline's exact pixels, the clip's frame counts, a pixel's own luma and chroma, and the stats sampler | of the 12, 9 only widen the privacy filter's whole-block shortcut, whose per-pixel path gives the same picture, and 3 change a log line or an encoder error no test can raise |
  | `capture.rs`, the capture loop around a real camera (the weakest) | 87 of 124 | 98 | the arrival photo's privacy filter (decoded, so a swapped filter fails too), the bus's error and end of stream, the restart pause and a zero width | needs a Pi camera, a V4L2 device or a loaded model, or changes a log line |

  The webhook tests fail after 10 s instead of hanging when no event comes, which a mutant that sends nothing first showed: each one cost the run its 300 s timeout. **Keep `--jobs 1` or give each job its own target dir**: two jobs sharing `CARGO_TARGET_DIR` overwrite each other's test binary and misreport, and so do two runs at once (a check run beside a long one reported caught mutants as missed on 2026-10-15), so each run gets a target dir of its own. The three arrival files take about 75 minutes on a Pi 5, a `--re` over the changed functions minutes.

### The cat cam service

`kataglyphis_observant` is one process doing everything an ObservANT board needs, so a
board runs it as one service unit. How to run and configure it:
[`docs/observant.md`](docs/observant.md).

- **The camera is chosen, and chosen again.** `camera = "auto"` (the default) takes a
  Raspberry Pi camera through `rpicam-vid` piping raw I420 when `rpicam-hello
  --list-cameras` lists one, which keeps the host's own libcamera whatever its ABI;
  otherwise a CSI camera libcamera lists, then a UVC webcam (`v4l2src`, MJPEG only when
  it offers no raw format), then Media Foundation or kernel streaming on Windows. The
  Pi's `rp1-cfe`, `unicam` and ISP nodes never qualify. With no camera it streams a test
  pattern and probes again every 30 s. A capture that ends, errors or delivers nothing
  for 10 s is rebuilt with backoff, so an unplugged or late camera needs no restart.
- **Probes start only the camera device providers** (`libcameraprovider`,
  `v4l2deviceprovider`, `mfdeviceprovider`, `ksdeviceprovider`); a device monitor would
  start every provider, NDI and PipeWire included, on each probe.
  - **And only for the choice that needs them.** When `rpicam-hello` lists a camera, no
    provider starts: `libcameraprovider` loads the host's libcamera into the service,
    and on himbeere2 (2026-10-05) that brought the host's abseil, TensorFlow Lite and
    lttng into a process that runs its own glibc.
  - `test` and `image:` probe nothing.
- **`rpicam-vid` belongs to a guard** (`RpicamProcess`), which kills and reaps the
  process however the capture setup fails. Before it, a missing `rawvideoparse` (the
  `rawparse` plugin, not `videoparsersbad`) left one zombie per retry.
- **Inference is capped at `inference_fps`** (default 2) frames a second; `0` removes the
  cap. Uncapped, the detector (YOLO then) held a Pi 5's four cores at 380 %. The stream
  keeps the camera's rate.
- **Frames stay I420 from capture to output** (`frame.rs`). A Pi camera delivers I420 and
  the encoder takes it, so neither end converts. Boxes are drawn into the three planes,
  the inference thread converts only the frames it takes to RGBA (BT.601 limited range,
  what `videoconvert` assumes for these caps), and `/snapshot.jpg` converts on request.
  Asking the capture for RGBA again would convert every frame: on himbeere2 (2026-10-12)
  I420 took the idle service from 13 % to 9 % of a core, its capture thread from 5.2 %
  to 1.7 %.
- **A still, empty room idles the model** (`motion.rs`). At most once an inference period
  the capture compares a thumbnail of 8x8-block mean luma (80x60 at 640x480) with the
  last one. A frame goes to the model when at least 0.5 % of the blocks moved by more
  than 12 levels, once `idle_inference_fps` (default 0.2) allows, or, while a cat is in
  view or the last run saw one, once a `still_period` (twice the full period, since
  2026-10-13): a still cat, asleep or a false one, costs half the runs, and leaving is
  motion, which restores the full rate. `Presence` lingers past the still period, so one
  missed run never ends a visit; himbeere2's drying rack, a false cat at 0.34, went from
  69 % to 36 %. An `idle_inference_fps` at or above `inference_fps`, or an uncapped
  model, turns the gate off.
  - On himbeere2 (2026-10-11, 2 Hz, 60 s per row) a still scene without a cat took 12 %
    CPU and 11 runs, against 74 % and 100 with the gate off. The cat photo and the moving
    test pattern kept the full rate. `/healthz` counts the runs (`model_runs`).
  - A black frame, every block at luma 20 or less, skips the idle run too: a camera
    without IR sees nothing at night. Light coming on is motion. In himbeere2's dark room
    (2026-10-12) the service went from 9 % to 2 % of a core, and the model ran once in a
    minute instead of eleven times.
- **ORT runs two threads that sleep between runs.** `main` sets `KATAGLYPHIS_ORT_SPIN=0`
  unless the caller set it, and `inference_threads` (default 2, `0` one per core) becomes
  `KATAGLYPHIS_ORT_THREADS`, both before anything starts a thread. ORT's own default,
  four threads that spin-wait between runs, is for back-to-back inference, and the
  inference crate keeps it for every other caller. Measured on himbeere2 (Pi 5,
  2026-10-11; ANThology's photo at 2 Hz, 45 s per row, the live service beside it):

  | ORT setting | process CPU |
  | --- | --- |
  | spinning, 4 threads (ORT's default) | 183 % |
  | sleeping, 4 threads | 106 % |
  | sleeping, 2 threads (the default) | 81 % |
  | sleeping, 1 thread | 71 % |
  | no inference | 10 % |

  Every row found both cats, best score 0.72. Letterboxing (`KATAGLYPHIS_PREPROCESS`)
  scored the same photo 0.61, so the preprocessing stays the crate's default, stretch.
- **A cat counts after two hits in a row** (`presence.rs`), and leaves once no hit came
  for 3 s, or 2.5 inference periods when that is longer. Until then no box is drawn and
  nothing is logged. A lone detection near `score` is mostly noise: of the 72 arrivals
  the old rule logged on himbeere2 (2026-10-09 to -11, 2 Hz), 52 were a single hit, and
  42 of the 72 scored 0.2x. The journal keeps its words, `cat in view: …` on arrival and
  `no cat in view (stayed N s)`, because OmniAccelerANT's package tests grep the first.
- **It serves the page itself.** `http_port` (8080) serves `web_root`, the OmniAccelerANT
  web build, with the MIME types and COOP/COEP that `serve.sh` adds, plus an SPA fallback
  and ETags; a revalidation reads no file. A client that accepts gzip gets the
  compressible files over 1 KB gzipped (`GzipCache`): compressed on first request, kept in
  memory while the file is unchanged, with an ETag of their own and
  `Vary: accept-encoding`. On himbeere2 (2026-10-12) the boot's main files went from 20.2 MB to
  7.5 MB; the first request for the 7 MB CanvasKit spent 614 ms compressing, later ones
  2 ms. It also answers `/healthz` (camera, model runs, the cat's presence),
  serves `/snapshot.jpg` (the latest frame, boxes included, encoded on request) and forwards
  `/webrtc-ws` byte for byte to the signalling server, which listens on `127.0.0.1` by
  default now. `run-producer-pi.sh` keeps the old shape (`--signalling-host 0.0.0.0
  --http-port 0`) because `serve.sh` is that setup's web half.
- **The settings page changes `rotate`, `score` and `name` at run time** (2026-10-13).
  `POST /settings` (JSON only, any subset) goes through `Status::apply`, which validates the
  whole update before it stores any of it; `settings.json` in the state directory keeps what
  the page set and wins over the config file, key by key. Each reader takes the live value:
  the capture loop compares the angle each frame and sets the method of its `videoflip`,
  which is always in the pipeline (a pass-through at 0), so a turn needs no rebuild; the
  inference thread reads `Status::score` each run; the notifier reads `Status::name` each
  event. `webrtcsink`'s `meta` keeps the name it started with. The frame caps say square
  pixels, so `videoscale` borders a quarter turn instead of stretching it; do not drop
  `pixel-aspect-ratio` from `frame::caps`.
- **Who may read a cat cam from another origin** (2026-10-14): only the origins in its
  `peers`, and only the reads (`web::handle`'s `reads`). The page's COOP/COEP stay, so a
  peer's pictures need `Cross-Origin-Resource-Policy: cross-origin` as well as CORS. Acting endpoints never get CORS headers; `own_page` still guards them.
- **What a cat cam on a LAN must refuse** (2026-10-13). `web::own_page` guards the two
  endpoints that act, `/webrtc-ws` and `POST /settings`: a browser's `Origin` must equal the
  request's `Host`, and the host must be an IP, a dotless name, a local suffix (`.local`,
  `.lan`, `.home`, `.internal`, `.home.arpa`, `.fritz.box`) or in `allowed_hosts`. Reads stay
  open. A reverse proxy under a public name needs its name in `allowed_hosts`.
- **A helper only a `#[cfg(unix)]` path uses needs `#[cfg(unix)]` itself** (2026-10-14). The
  Windows lanes lint with `clippy --all-targets -- -D warnings`, and there an unused struct is
  an error: `I420Layout` (the rpicam path's) failed Windows arm64 run 38496162066 while every
  Linux lane passed. The Windows lint also runs on a Linux box, in about 5 minutes against
  the Windows lane's hour: `bash scripts/linux/clippy-windows.sh` as root in `:latest`
  (`--package` for another crate) installs mingw for ring's C, adds `x86_64-pc-windows-gnu`
  and runs clippy with `-D warnings` for it.
- **`rpicam-vid --codec yuv420` pads its rows** (2026-10-14) to 128 bytes, so a 480-wide
  frame has 512-byte rows, 276480 bytes. `I420Layout::rpicam` hands `rawvideoparse` the
  strides; a width that is a multiple of 128 (640) needs none, which is why 640x480 never
  showed it. Read as tight rows, the picture turns into green stripes and the motion gate
  sees motion everywhere.
- **What may start a visit** (2026-10-14, `capture::counted`): only a detection with motion
  near it (`motion::Moving`, from the pacer's looks), after `presence::outside` dropped the
  spots marked "not a cat". Test inputs that never move, such as `image:FILE`, still arrive
  once: the pacer's first look stamps every block. With the motion gate off there is no
  `Moving`, and every detection counts.
- **Per-arrival files** (`status.rs`, `Kept`): clips (`clip-<t>.mp4`) and pictures
  (`arrival-<t>.jpg`) both keep the last five by the visit log's arrival time, so `?t=`
  names the visit's own; set them after `set_cat`, which writes that time.
- **A webhook hears about every visit** (`notify.rs`). With `notify_url` set, each
  arrival and departure is POSTed as JSON (`ureq` with rustls, so https too) from the
  `cat-notify` thread, so a slow receiver cannot slow detection. Before the
  arrival goes out, `/arrival.jpg` is set to the frame the model confirmed it on, boxes
  drawn. Logs name only the URL's origin: a Home Assistant webhook's path is its secret.
- **The cat outlives a restart.** When systemd sets `$STATE_DIRECTORY` (the unit's
  `StateDirectory=`), every arrival and departure writes `visits.json` (last seen, visits,
  last score) and the arrival pictures there, each through a rename so a crash leaves no torn
  file, and the next start reads them back (a `cat.json` from before the rename too). Without
  it nothing is kept. Where no systemd provides it, the installer does: the Windows exe's
  `install::adopt` sets it to `%LOCALAPPDATA%\omni-observant` (taking over an older
  `omni-accelerant-catcam`), OmniAccelerANT's flatpak launcher to its `$XDG_STATE_HOME`.
- **The network is pinned.** Media uses UDP `ice_port_min..ice_port_max`
  (40000-40099), set on every viewer's ICE agent from `consumer-added`, and STUN is off
  unless `stun_server` names one, so a LAN stream needs nothing outside it.
- **Settings**: where they come from, unknown keys, `--print-config` and
  `inference = "auto"`: [`docs/observant.md` § *Settings*](docs/observant.md#settings).
- **A Windows install is one folder, and the exe finds everything in it** (`install.rs`).
  - When `lib\gstreamer-1.0` sits beside the exe, `install::adopt` sets the variables the
    folder implies, first thing in `main`: `GST_PLUGIN_PATH`, an empty
    `GST_PLUGIN_SYSTEM_PATH[_1_0]`, `GST_PLUGIN_SCANNER`, `GST_REGISTRY` under
    `%LOCALAPPDATA%`, `ORT_DYLIB_PATH` and `KATAGLYPHIS_ONNX_MODEL` (`models\deim_dfine_n_coco.onnx`).
  - A variable the caller already set wins.
  - On Linux the bundle's `observant` launcher does the same job, so `adopt` is a no-op there.
  - OmniAccelerANT's `scripts/windows/observant/Package-ObservANT.ps1` builds that folder
    and the MSI around it.
- **Logs and exit:** it logs at info (`KATAGLYPHIS_LOG_LEVEL`) and quiets the signalling
  server's per-message INFO (`WEBRTCSINK_SIGNALLING_SERVER_LOG=warn` unless set). It
  stops cleanly on SIGTERM and exits non-zero only when the output pipeline fails.

### The cat producer on a Raspberry Pi 5

`scripts/linux/observant/run-producer-pi.sh` builds and runs `crates/observant` from the
family CI image against a Pi 5 CSI camera. It moved here from OmniAccelerANT on
2026-09-15 under decision D12, because the crate it drives is this repo's;
OmniAccelerANT keeps a pointer and still owns the web half
(`scripts/linux/observant/serve.sh` over there).

```bash
scripts/linux/observant/run-producer-pi.sh --build       # first run: cargo build in the image
scripts/linux/observant/run-producer-pi.sh               # start the producer
scripts/linux/observant/run-producer-pi.sh --libs-only   # refresh build/observant/hostlibs
```

Two things it deliberately does **not** spell out, because ANTfrastructure owns both and
a copy here goes stale:

- **The image reference**: it asks `third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh`,
  which reads the fleet's `versions.env`. A literal `ghcr.io/…:latest` in a tracked `*.sh`
  is what the lint lane's CI-image-ref gate fails on.
- **`LD_LIBRARY_PATH`**: the runner hands in only `/hostlibs`, and the rest comes from the
  image. Its normal entrypoint restores GStreamer, appends `/opt/libcamera` and puts
  `${GCC_PREFIX}`'s runtime first; the container prologue appends what a caller-set value
  dropped (`/usr/local/lib`, OpenCV, FFmpeg) from the image's own
  `/opt/scripts/03-media/final/media-env.sh`, the same file the Dockerfiles source. The
  retyped list this replaced had `/opt/gcc-16.2.0` hard-coded, so a GCC bump upstream
  would have silently dropped the C++ runtime out of the search path.

The script's own contribution to the loader path is only `/hostlibs` (the host's
Raspberry Pi OS libcamera closure, which must win over the image's older upstream copy)
and the host multiarch directory, appended last. `collect_hostlibs` copies libcamera,
libcamera-base, libpisp and the PiSP IPA under their SONAMEs and the hub's
`elf-closure.sh --ldd` adds what they load but the C runtime (2026-10-19); a library ldd
cannot find stops the collection, where the old walk left it out.

**It runs through the image's entrypoint, not around it** (2026-09-29). Before
ANTfrastructure CON23 the entrypoint's `libcamera-env.sh` *prepended* `/opt/libcamera`
over the caller's `LD_LIBRARY_PATH`, so this runner started `bash` with `--entrypoint` to
keep `/hostlibs` ahead. Since `:latest` of 2026-09-29 the image's libcamera goes *after*
the caller's entries and GCC's runtime before them, so the bypass is gone from all three
`nerdctl run`s. The prologue refuses an older image that still puts `/opt/libcamera`
ahead of `/hostlibs`, with `pull :latest` in the message. It was proven under QEMU
first: the arm64 child, the runner's recorded `nerdctl run` with an empty stand-in
`/hostlibs`, gives GCC → GStreamer → `/hostlibs` → `/opt/libcamera` → media paths → host
multiarch, `libgstlibcamera.so` resolves `libcamera.so.0.7` from a stubbed `/hostlibs`,
and `gst-inspect-1.0 libcamerasrc` and `webrtcsink` load. The Pi 5 board run followed on
2026-10-06: `--build`, then a stream from himbeere2's CSI camera (`CHANGELOG.md`).

### Build & test in the Stevedore Windows container

The driver, `scripts\windows\container\Invoke-StevedoreBuild.ps1`, its switches and what
it leaves behind: [`docs/windows.md`](docs/windows.md). It runs the in-container scripts
(`Build-RustAll.ps1`, `Test-RustAll.ps1`) in the family Windows CI image through the hub's
`Invoke-RepoContainerBuild.ps1`, which streams the repository in, `third_party/` included,
so they import ANTfrastructure modules through `Resolve-BuildModule.ps1` like every other
script here.

**No image reference is written in this repository, and that includes this file.** The
driver's `-Image` parameter defaults to empty and is filled in by ANTfrastructure's
`Get-CiImageReference -Windows`, which composes `IMAGE_REGISTRY_PREFIX` +
`CI_IMAGE_WINDOWS_TAG` from the submodule's `linux/scripts/01-core/versions.env`, the
fleet's single owner of both CI refs, so a tag bump lands in one file in one repo and
arrives here with no edit. Pass `-Image` to override for a one-off. Prose is not exempt:
a reference typed into a table or a README goes stale like one typed into code, and
`verify_ci_image_refs.py` check D fails a build on a copy in any tracked `*.sh` /
`*.ps1` / `*.psm1` or workflow YAML.

**The host-side mechanics are upstream's.** Why writes through a bind mount fail while
reads succeed, how to allow the Dev Drive filters, what `--isolation process` does to the
CPU count, the wcifs teardown lock, the transient hcsshim client-pipe drops, and why the
Windows lane is Stevedore's `docker.exe` rather than nerdctl:
[`docs/windows-builds.md`](third_party/ANTfrastructure/docs/windows-builds.md) and
[`docs/windows-container-build-performance.md`](third_party/ANTfrastructure/docs/windows-container-build-performance.md).
Read those before changing the driver. **Do not copy their commands back into this
file**: the last copy of the `fsutil devdrv` line that lived here was malformed and
stayed that way through several edits.

### Continuous integration

Eleven workflow files: the build lanes, the gate lanes, `docs.yml`, `on-request.yml`,
`release.yml` and the reusable `reusable-linux.yml`. The five build lanes, one file per platform + arch,
run inside ANTfrastructure images rather than on the runner. The gate lanes pull no
image, and **three of the four gate jobs run ANTfrastructure's code**, two a reusable
workflow and one its `run-pester-suite` action; the fourth runs this repo's script suites.

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
[the hub's `branch-and-release-strategy.md`](third_party/ANTfrastructure/docs/branch-and-release-strategy.md).
Here `on-request.yml` offers four lanes: `[ci windows]` runs both Windows lanes, `[ci arm64]`
both arm64 ones, `[ci linux]` arm64 and riscv64, `[ci riscv64]` and `[ci x64]` one each, and
`[ci all]` all four. A change to Windows-, arm64- or riscv64-only code asks for its lane in
its commit message. `[build-arm]` and `[build-win]` are dead tokens.

**Every push to `main` is a release** (`release.yml`): all five lanes once more, then
`v<version>` with the Linux packages for both arches (each flatpak named for its arch), the
Windows MSI, MSIX and portable zip for x64 and arm64, a `SHA256SUMS` and the version's
`## [<version>]` CHANGELOG section. The version check and the release job are the hub's
[`release-publish.yml`](third_party/ANTfrastructure/docs/branch-and-release-strategy.md#the-reusable-release-workflow),
whose collector picks the files by the `collect:` options in `release.yml`. The version lives in `Cargo.toml`'s `[workspace.package]` alone (owner, 2026-10-15:
2.3.4, and `VERSION.txt` is gone). `gh workflow run release.yml --ref develop -f
dry-run=true` is the full round a merge needs, and publishes nothing.

**File names, display names and concurrency groups are the family's**
([§ *Workflow file names and display names*](third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#workflow-file-names-and-display-names),
[§ *How a repository implements it*](third_party/ANTfrastructure/docs/branch-and-release-strategy.md#how-a-repository-implements-it)).
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
  and `media-check` (`dist/windows-x64/bundle/kataglyphis_cli.exe`).
- **arm64** (`windows-arm64-cross.yml`): `Build-Windows.ps1`, in the family image's arm64
  bundle, then `Stage-CrossTests.ps1`, which builds the whole workspace's tests for arm64,
  renderer included, and stages them with their DLL closure, the product's GStreamer
  plugins and the renderer's fixtures. The hub's arch gate then grades `dist/windows-arm64`
  and the test tree, and `windows-11-arm` runs `kataglyphis_cli.exe --help`, `stats`,
  `onnx-runtime` and `media-check` natively, then every staged test with
  `KATAGLYPHIS_REQUIRE_GPU=1` (hub CON43), the renderer's one test at a time. In parallel,
  WARP itself crashes: dispatch 37422053110 (2026-10-06, `parallel-renderer-tests`) stopped
  `ibl.exe` with `0xC0000005 STATUS_ACCESS_VIOLATION`, faulting module `d3d10warp.dll` at
  offset 0x80528 (WARP 10.0.26100.9278), as run 36889467167 had stopped `headless.exe`.
  Serially all of them pass (run 36907253720), and the whole job takes 2.5 min. The fault is
  in Microsoft's driver, not this repo; re-dispatch with the input after a runner image
  update to see whether it is gone. No arm64 Windows container image exists, so that
  job is the only place an arm64 binary of this repo executes.

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

**Nothing generated may be tracked.** The hub's `shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1` runs `Get-TrackedIgnoredFile` and `Get-TrackedGeneratedArtifact` over this root; only the generated-output pathspecs are local, one per line in `scripts/windows/generated-artifacts.txt` (since 2026-10-19; a local copy of the suite held them before). Locally, from the repo root: `Invoke-Pester -Path third_party/ANTfrastructure/shared/windows/tests/Repo.GeneratedArtifacts.Tests.ps1`. A `.gitignore` rule stops a file from being *added* and does nothing once a path is in the index, which is why `git rm --cached` has been needed twice here (build logs under `logs\windows\`, and the flatpak repo). Mind the pathspec form: `'**/__pycache__/*'` matches, `'**/__pycache__/'` matches **nothing**, and the suite refuses it.

**The lint lane runs with `--ratchets` on** since 2026-09-15. On top of the six always-on gates that adds the docs cross-reference gate plus eight measurement gates (code size, complexity, dead functions, comment size, stdout returns, masked declarations, trailing conditionals, and a shellcheck *warning* ratchet), each graded against a freeze file at the repo root. Three of them carry rows (`comment-size.allow`, `code-complexity.allow`, `dead-functions.allow`), seeded from the first run; the rest are absent, which the gates read as a zero baseline. **The contract is two-way**: a new offender fails, and so does an entry that is no longer over the limit, so fixing one means deleting or updating its row in the same change. The docs gate has no freeze file and never will: a `docs/…md` pointer in code either resolves from the repo root (or, when it starts `../`, from the file) or it is a finding.

### The riscv64 lane

**Cross-built on amd64, tested under QEMU** (owner decision 2026-10-01, hub CON48); why,
the sysroot, the wrappers and QEMU are
[the hub's `riscv64-cross-test-lanes.md`](third_party/ANTfrastructure/docs/riscv64-cross-test-lanes.md).
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
  [the hub's traps](third_party/ANTfrastructure/docs/riscv64-cross-test-lanes.md#traps-each-found-by-a-failing-run)
  say why.
- **pkg-config reads the amd64 image's `.pc` files here** (hub CON130): gstreamer-sys's link paths
  are `/opt/riscv64-sysroot/opt/gstreamer/lib/x86_64-linux-gnu` and `.../lib`, and it links only
  because the second holds the riscv64 libraries (2026-10-20). Likewise the image's
  `GST_PLUGIN_PATH` starts with `/opt/gstreamer/lib/x86_64-linux-gnu/gstreamer-1.0`, so each test
  process fails to load about 300 amd64 plugins (warnings) before the sysroot's riscv64 ones load.
  Both fixes are the hub's.
- **The root crate's real-model tests take turns** (`model_turn` in `src/lib.rs`, 2026-10-20): on
  four cores three model loads at once under QEMU kept the webcam engine's first event past its
  60 s (run 39487523314); one at a time all 25 pass in 168 s on 4 CPUs locally.
- **A spawned host program is not remapped** (`execve`, the same traps). A bare `sleep`
  exited at once there, which made a test of
  `camera::run_with_timeout` see output instead of a timeout (9e0a401 to 8a5337b, red); the
  test now goes through `sh -c`, and the function returns `None` for any failed exit.

### Lane facts that cost debugging time

- **The arm64 lane runs natively on `ubuntu-26.04-arm`**, on every push from 2026-09-24 to 2026-10-15 and on request since. It was opt-in via `[build-arm]` for runner minutes before that, never because it could not pass: `:latest` (then `:latest-cross`) has been a multi-arch index since 2026-09-04.
- **Linux artifacts are named by `VERSION`, not by `github.ref_name`.** On a pull request the ref name is `<number>/merge`, `upload-artifact` refuses a `/` in a name, and every PR run of the Linux lane went red at *Upload all artifacts* with the build, tests and package green (run 35752031200, 2026-09-22). Fixed on 2026-09-24, when the lane started running on every PR on both architectures.
- **Every cargo step of both Linux lanes is one named step of one script**, `scripts/linux/ci-container-steps.sh` (`ort-chain-only`, `debug`, `security`, `fmt-clippy`, `test`, `coverage`, `bench`, `release`; `docs` builds locally what `docs.yml` builds). Each step used to inline its own `bash -lc 'set -e; git config --global --add safe.directory /workspace; bash third_party/.../cargo_<x>.sh'`, the same prologue eight times. Reproduce any step with `bash scripts/linux/ci-container-steps.sh <step>` inside the image; `reusable-linux.yml` runs exactly that line. The one container step outside it is packaging, `scripts/linux/package-linux.sh`, which runs `--privileged` for flatpak-builder. Until 2026-10-20 `linux-x64.yml` also checked six feature sets in a `feature-matrix` job; with no features left, the lane's own build covers them.
- **The docs site is `docs.yml`'s, not a lane's** (2026-10-19). It calls the hub's `build-docs.yml` with `builder: rustdoc`: `cargo_build_doc.sh` documents all 12 crates into `target/doc`, the hub adds its `.htaccess` and 404 page, the built-site link gate checks every internal link and anchor, and a push to `develop` or `main` publishes to <https://rust.jonasheinle.de> under a never-cancelled group. A red lane no longer holds the site back; a dead link fails it. The 24 keys in `docs/site-links.allow` are rustdoc's copies of relative links in blanket impls' docs (`tracing`'s `WithSubscriber`, burn's `Module`); the freeze is two-way, so a key that resolves again must go. Inputs, secrets and the gate: [the hub's `docs-sites.md`](third_party/ANTfrastructure/docs/docs-sites.md).
- **The container runs as uid 1001, not root.** `apt-get` fails with `Permission denied`, so a workflow step cannot install system packages: whatever the image lacks, it lacks. The image's `CARGO_HOME` (`/usr/local/cargo`) and `RUSTUP_HOME` belong to uid 1001, which the hub's consumer-image contract requires (row 2 of `docs/consumer-image-contract.md`): `find /usr/local/cargo /usr/local/rustup ! -uid 1001` finds nothing in `:latest`, and a `cargo check` fills the registry there (2026-10-19). The lane jobs set `-e CARGO_HOME=/tmp/cargo-home` only because their cache mount keeps the registry there between steps.
- **The Windows x64 lane pins its cargo tools** (the arm64 cross build runs no audit/deny). `scripts/windows/Build-Windows.ps1` installs cargo-audit and cargo-deny with `--version`, read from `$env:CARGO_AUDIT_VERSION` / `$env:CARGO_DENY_VERSION` (baked into the image) and falling back to the submodule's `versions.env` through `ConvertFrom-VersionsEnv`; unresolvable **throws**. Unpinned, `cargo install` takes whatever crates.io serves that minute, so a new advisory-db schema turns the lane red with no commit to bisect. No try/catch wraps the install any more either: a swallowed failure left both gates running whatever was on PATH.
- **`cargo_security_checks.sh` is a gating step, not an advisory one.** The *Security checks (cargo audit + cargo deny)* step in `reusable-linux.yml` (so in both Linux lanes) runs ANTfrastructure's `linux/scripts/02-toolchain/rust/cargo_security_checks.sh`, and a finding fails the lane. It reads **two** ignore lists: `.cargo/audit.toml` (cargo-audit) and `deny.toml` `[advisories].ignore` (cargo-deny). For a crate that is built they hold the same ids, since an id on one list only leaves the other tool reporting it; paste and ttf-parser are the exception, built but no longer reported by cargo-deny 0.20.2 (2026-10-20), so `audit.toml` alone lists them. cargo-audit alone reads `Cargo.lock`, which also locks optional dependencies no feature enables: an advisory in such a crate (zip 0.6.6 via tch, 2026-10-17) goes on `audit.toml` only, with the `cargo tree -i <crate> --all-features` proof, because cargo-deny never sees it and calls the ignore stale. Prefer an upgrade over an ignore: `chacha20` 0.10.1 (yanked) and `stable-vec` 0.4.2 (unsound) both had one, so neither is on either list.

Lint the workflows locally with the submodule's pinned, SHA-verified actionlint (works from Git Bash on Windows):

```bash
bash third_party/ANTfrastructure/linux/scripts/lint-workflows.sh .
```

The trailing `.` is load-bearing: without it the script lints ANTfrastructure's own workflows instead of this repo's, and reports green either way.

### Dependency upgrades

Renovate, run as a **local CLI** (`bash scripts/linux/renovate-local.sh`); its commands,
what `--apply` writes, WSL and the token: [`docs/updates.md`](docs/updates.md). Every lane
above runs its gates, drivers and modules out of the `third_party/ANTfrastructure` pin
(§ 3), so that gitlink drifting is a silent change to every gate, and nothing watched it
before this wrapper existed. No Renovate GitHub App or workflow runs it.

**A run reports. `--apply` only when the owner asked for it in that turn** (owner
directive 2026-09-11, a family rule the hub's `AGENTS.md` keeps). The family rationale:
[`third_party/ANTfrastructure/docs/dependency-updates.md`](third_party/ANTfrastructure/docs/dependency-updates.md).

## 6. Docs owned by this repo

**Write docs like a developer: short and precise** (owner, 2026-10-19). Say what a thing does,
how to run it and what breaks; no filler, no marketing, no repetition.

`AGENTS.md` (this file), `README.md`, `BACKLOG.md`, `CHANGELOG.md`, the guides in
`docs/`, `crates/webgpu_renderer/README.md`, the renderer's design documents in
`crates/webgpu_renderer/docs/`, and `third_party/egui-winit-0.36.2/PATCHED.md` (the
vendored crate's provenance and removal steps). Rustdoc is published to
<https://rust.jonasheinle.de> by `docs.yml` on every push to `develop` or `main`. Update
the docs in the same change as the behaviour they describe.

**The README is a short overview** (owner, 2026-10-19: the family's README rule,
[`docs/repository-family.md` § The README rule](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/docs/repository-family.md#the-readme-rule)).
Everything else is a guide in `docs/`, one row each in the README's *Documentation*
table: [`getting-started.md`](docs/getting-started.md), [`crates.md`](docs/crates.md),
[`testing.md`](docs/testing.md), [`running.md`](docs/running.md),
[`observant.md`](docs/observant.md), [`windows.md`](docs/windows.md),
[`packaging.md`](docs/packaging.md) and [`updates.md`](docs/updates.md). A new guide
gets a row there too. **The README links with absolute GitHub URLs**, because it is
also the `oxidant` crate's rustdoc front page (`#![doc = include_str!("../README.md")]`
in `src/lib.rs`), where a relative link resolves under rust.jonasheinle.de and is a 404.
For the same reason every code block in it carries a language (`bash`, `pwsh`): an
untagged one is compiled as a doc test. The rustdoc build publishes no `docs/` page
([`getting-started.md` § API docs](docs/getting-started.md#api-docs)).

### Where the renderer's design documents live

**They moved on 2026-09-15** (decision D6): this repo owns the renderer, code and
documentation. Three pages left `BeschleunigerBallett/docs/` for
`crates/webgpu_renderer/docs/`, and BeschleunigerBallett kept a pointer file at each old
path, not a copy. Four pages describe both renderers and stayed there.

| Document | Owner |
| --- | --- |
| `renderer-bounds-invariant.md` | **here**, `crates/webgpu_renderer/docs/` |
| `webgpu-renderer-roadmap.md` | **here**, `crates/webgpu_renderer/docs/` |
| `webgpu-gltf-rust-plan.md` | **here**, `crates/webgpu_renderer/docs/` |
| `gpu-golden-testing.md` | stays in BeschleunigerBallett |
| `model-loading.md` | stays in BeschleunigerBallett |
| `shader-sharing.md` | stays in BeschleunigerBallett |
| `webgpu-srgb-audit.md` | stays in BeschleunigerBallett |

**The two halves are referenced differently, on purpose.** The three that live here
are referenced by relative path: `docs/<name>.md` from `crates/webgpu_renderer/README.md`,
and relative to the file in code, `../docs/…` in `src/lib.rs` and `../../docs/…` twice
in `src/render/bounds.rs` (the docs cross-reference gate resolves a bare `docs/…` in code
from the repo root). Those paths resolve in every checkout, which is what the move
bought: this repo is built standalone, from OmniAccelerANT and from BeschleunigerBallett
(see [Consumers](#consumers)), and before the move a relative `docs/` prefix meant *the
superproject's* root and pointed at nothing in two of the three. The four that stayed
are absolute URLs
(`https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/<name>.md`) for
the same reason, and must stay absolute. `bounds.rs` calls
`renderer-bounds-invariant.md` the checklist for not repeating eight identical bugs, so
a dead link there costs more than tidiness.

### Large tracked files

One large file is tracked deliberately, not in git-lfs; `large-files.allow` freezes it
for the hub's `large-files` gate. **Decision D4, "the history is not being rewritten", is
superseded** (owner, 2026-10-17): the family's history rewrite took this pack from 63 to
12 MiB. What it removed: [the hub's `data-repositories.md`](third_party/ANTfrastructure/docs/data-repositories.md#what-moved-and-why);
a checkout from before follows with
[§ *After a history rewrite*](third_party/ANTfrastructure/docs/adopting-in-a-new-project.md#after-a-history-rewrite).

| File | Size | Why it is tracked |
| --- | --- | --- |
| `images/Rust.webp` | ~2 MiB | The README hero image, rendered by GitHub; the 7.5 MiB GIF it replaced (2026-10-17) left the history in the rewrite. |

**Models are data, not code, and are never tracked** (owner, 2026-10-14). The AGPL-3.0
`yolov10m.onnx` (59 MiB) left the tree then, and the history in the 2026-10-17 rewrite.
Its successor is DEIM-D-FINE-N (Apache-2.0, 15 MiB), built from its pinned upstream and
released by [ModelANT](https://github.com/Kataglyphis/ModelANT).
`resources/models/models.lock` names each file with its URL and SHA-256, and
`scripts/linux/fetch-models.sh` (Windows: `scripts/windows/Get-Models.ps1`, the *Fetch
Models* build step) puts them beside it, its license and notice included. Both only run the
hub's [`fetch-data-lock.py`](third_party/ANTfrastructure/docs/data-repositories.md#the-hubs-fetcher)
on that lock (2026-10-19), so Windows needs `python` on `PATH`, as both Windows images have
it. A checked copy waits in `KATAGLYPHIS_MODEL_CACHE`, else `~/.cache/kataglyphis-models`
(Windows: `%LOCALAPPDATA%\kataglyphis-models`), so only the first run needs the network. A new
model goes into ModelANT first, and only the lock changes here.

Everything else tracked is small: the renderer's glTF/GLB/KTX2 test assets are a few
hundred kilobytes each, and stay that way on purpose.

**Nothing new joins them.** `.gitignore`'s *Large binaries* block excludes model weights,
video, packaging artefacts, archives and signing material. A file kept despite it gets a
negation rather than a narrower pattern, so the rule still bites if the file is moved or
renamed (e.g. `!/resources/models/models.lock`). If another large file really is
necessary, add it to the table above in the same commit that force-adds it.

`logs/windows/` used to hold 15 committed Windows build logs (~2.4 MiB) that
`.gitignore`'s own `logs/**/*` rule already covered; they are untracked now and the
files stay on disk. `Build-Windows.config.psd1` still writes there.

### Conventions

- Version pins/single sources of truth follow the ANTfrastructure ecosystem; don't duplicate what the submodule documents — link to it.
- Never commit build outputs: `/target`, root `/debug`, `/profile`, `/release` are gitignored, and so is `logs/`.
