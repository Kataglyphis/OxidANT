# AGENTS.md

Guidance for AI agents (and humans) working in OxidANT.

Laid out on ANTfrastructure's six-section template
([`third_party/ANTfrastructure/shared/templates/AGENTS.md.template`](third_party/ANTfrastructure/shared/templates/AGENTS.md.template)), the same shape as
OrchestrANT, AccelerANTgine and ANThology. The rule that decides where a paragraph goes:
*would this still be true in a different project?* If yes, ANTfrastructure owns it and
§ 2 links to it. If no, it is written out in § 4.

**Dated history is not here.** Measured baselines and the post-mortems of the
2026-08-07 packaging, graphics-stack and tract migrations live in
[`CHANGELOG.md`](CHANGELOG.md). This file is what to do now; that file is what
happened when.

## 1. What this project is

The Kataglyphis family's **Rust workspace**: the crates two other repositories build as
a submodule, plus the gates, packaging and practices they inherit with them. Started as
a project template, and the scaffolding is still here — but the crates have real
consumers, which is what makes renames expensive (see [Consumers](#consumers)).

Cargo workspace (`Cargo.toml` at the root is both the workspace and the root package `oxidant` — a lib with `cdylib`/`staticlib`/`rlib` crate types plus the feature-gated `burn-demos` bin):

- `crates/core` — core config/detection/logging (`kataglyphis_core`)
- `crates/telemetry` — resource monitoring (`kataglyphis_telemetry`; has the unit tests)
- `crates/inference` — ONNX backends, feature-gated (`onnx_tract`, `onnxruntime`, `onnxruntime_directml`, `onnxruntime_cuda`)
- `crates/gui` — feature-gated GUI (`gui_windows`, `gui_linux`, `gui_wgpu`, `gui_unix`)
- `crates/webgpu_renderer` - WebGPU (wgpu) glTF renderer, native + wasm32/browser (`kataglyphis_webgpu_renderer`): PBR, cascaded shadows, SSAO, bloom, skinning, animations, LOD
- `crates/media` — GStreamer capture, feature-gated (`gstreamer`)
- `crates/cat_webrtc` — cat-cam WebRTC producer (`kataglyphis_cat_webrtc`); consumer: OmniAccelerANT's Stream page. Its Raspberry Pi 5 runner is `scripts/linux/cat-stream/run-producer-pi.sh` (see [Build, run, test](#5-build-run-test))
- `crates/cli` — the CLI binary; its bin target is named `kataglyphis_cli` (read/stats/gui subcommands; `stats --path <file>`). It was renamed from `oxidant` on 2026-08-07 — see the pdb note below.
- `src/` — the root package: the flutter_rust_bridge surface for OmniAccelerANT (`src/frb_generated.rs`, `src/api/{onnx,simple,webcam}.rs`, `src/webcam_engine.rs`) plus the `burn-demos` bin
- `tests/` — root-package integration tests (`integration.rs`) and proptest fuzz tests (`fuzz_test.rs`)
- `third_party/ANTfrastructure` — git submodule and **the ground truth for every container and PowerShell concern**. See the section below before writing any helper.

### Consumers

Repositories that build this one as a submodule; a rename or a `[lib] name` change has
to be carried into each of them **in the same change**:

- [OmniAccelerANT](https://github.com/Kataglyphis/OmniAccelerANT) — the root package
  through flutter_rust_bridge (Cargokit, podspecs, the committed `frb_generated.dart`
  loader stem) and `crates/cat_webrtc` for its Stream page
- [BeschleunigerBallett](https://github.com/Kataglyphis/BeschleunigerBallett) —
  `crates/webgpu_renderer` and `crates/gui` through Corrosion. The import is
  `Src/CMakeLists.txt:74-80`: `corrosion_import_crate(MANIFEST_PATH
  ../third_party/OxidANT/Cargo.toml CRATE_TYPES staticlib CRATES oxidant)`, guarded by
  `if(RUST_FEATURES)`. It names the package `oxidant` and takes the **staticlib**, so
  `[package] name`, `[lib] name` and the `crate-type` list are all part of that
  repository's build.

## 2. What ANTfrastructure owns — links only

Anything to do with containers, Dockerfiles, CI plumbing or PowerShell belongs to the submodule. **Search it before writing a helper.**

**Do not re-derive host knowledge here — read it there.** Everything about
Stevedore, Rancher Desktop, wcifs/bindFlt and the container hosts is already
written down, in more depth than this file should carry. Start at
[`third_party/ANTfrastructure/docs/INDEX.md`](third_party/ANTfrastructure/docs/INDEX.md)
— it maps topic → owning document, so one hop survives upstream reorganisation.

The entries this repo reaches for most:

| Question | Document |
| --- | --- |
| How do I set up a Windows host for Stevedore? (services, `docker-users`, CNI nat conf, pwsh, gates) | `docs/windows-host-setup.md` |
| Why does a layer commit fail / what is process isolation doing? wcifs, bindFlt, the `ActivateLayer 0x20` bug | `docs/windows-builds.md` |
| Dev Drive filter setup, bind mount vs tar-pipe, container reuse | `docs/windows-container-build-performance.md` |
| How do I run Linux containers on Windows? | `docs/rancher-desktop-linux-containers.md` |
| Which image, which tag, which engine? | `docs/adopting-in-a-new-project.md` |
| Why did my lane not run? | `docs/ci-build-triggers.md` |
| How do I upgrade a dependency, and what will `--apply` refuse to move? | `docs/dependency-updates.md` |

When something in this file contradicts one of those, **the submodule wins**.
That has happened twice, both times because a procedure was retyped here instead
of linked: this file once claimed Dev Drive volumes *refuse* bind mounts (they
work; it is create-then-rename that fails), and it carried a `fsutil devdrv`
command that was missing `/volume` and split its filter list on a space — so it
could never have worked. Both are why § *Build & test in the Stevedore Windows
container* now links rather than restates.

### Linux containers locally (Rancher Desktop)

One sentence and a link, because the procedure is upstream's:
[`third_party/ANTfrastructure/docs/rancher-desktop-linux-containers.md`](third_party/ANTfrastructure/docs/rancher-desktop-linux-containers.md)
— the image is **always** `:latest` (the old `:latest-cross` name is retired), Rancher defaults to **containerd** so it is
`nerdctl --namespace default` rather than `docker`, and from Git Bash
`MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'` is mandatory or the mount argument is
mangled.

Two consumer-specific traps are *this* repo's, so they are written out in § 4.

### Reach for these before writing a helper

Every row below was written locally first and later found to already exist upstream —
usually in a better form, twice with a bug the local copy did not have. All paths are
relative to `third_party/ANTfrastructure/`.

| Need | Use | Defined in | Not |
| --- | --- | --- | --- |
| `docker.exe` discovery (Stevedore) | `Resolve-DockerExe` | [`windows/scripts/modules/WindowsContainerBuild.Reuse.psm1`](third_party/ANTfrastructure/windows/scripts/modules/WindowsContainerBuild.Reuse.psm1) | a hand-rolled candidate list |
| `--isolation process` and friends | `Get-ContainerIsolationArgs` | same file | inline flags |
| Container teardown | `Remove-BuildContainerSafe` | same file | `docker rm -f` (misses the wcifs teardown lock) |
| Bind-mount probe, artifact delivery | `Test-ContainerBindMount`, `Test-BuildArtifactsDelivered` | same file | assuming a green build delivered something |
| Stage, manifest, pack, sign one MSIX | `Invoke-MsixPackage` | [`windows/scripts/modules/WindowsMsix.Common.psm1`](third_party/ANTfrastructure/windows/scripts/modules/WindowsMsix.Common.psm1) | the ~100-line makeappx/assets/tokens/pack sequence this repo carried until 2026-09-15 |
| The version to stamp a package with | `Get-PackageVersion` | same file | reading `VERSION.txt` inline, once per packaging step, with a different fallback each time |
| SDK tools (makeappx, signtool) | `Resolve-WindowsSdkToolPath` | same file | `Get-ChildItem -Recurse` over the Kits tree |
| MSIX manifest tokens, XML escaping, placeholder PNGs | `Expand-XmlTemplateTokens`, `ConvertTo-XmlEscapedText`, `New-TransparentPng` | same file | `-replace` — see below — and local redefinitions |
| Config access | `Get-OrDefault`, `Get-ConfigValue` | [`windows/scripts/modules/WindowsConfig.Common.psm1`](third_party/ANTfrastructure/windows/scripts/modules/WindowsConfig.Common.psm1) | copies |
| Build logging and steps | `New-BuildContext`, `Invoke-BuildStep`, `Invoke-BuildExternal`, `Write-BuildLog*` | [`windows/scripts/modules/WindowsBuild.Common.psm1`](third_party/ANTfrastructure/windows/scripts/modules/WindowsBuild.Common.psm1) | ad-hoc `Write-Host` wrappers |
| Tool guards, workspace paths (pwsh) | `Assert-Command`, `Resolve-WorkspacePath` | [`windows/scripts/modules/WindowsScripts.Shared.psm1`](third_party/ANTfrastructure/windows/scripts/modules/WindowsScripts.Shared.psm1) | a second implementation |
| Logging inside a container | `Start-ContainerLog`, `Write-ContainerLog`, `Invoke-ContainerLoggedCommand` | [`windows/scripts/modules/WindowsContainerLog.Common.psm1`](third_party/ANTfrastructure/windows/scripts/modules/WindowsContainerLog.Common.psm1) | a `Say`/`Run-Logged` pair per script |
| CI version stamping (bash) | `version_util.sh --github-env` / `--resolve-ci` / `--normalize` | [`linux/scripts/02-toolchain/rust/version_util.sh`](third_party/ANTfrastructure/linux/scripts/02-toolchain/rust/version_util.sh) | re-reading VERSION.txt yourself |
| In-container cargo steps | `cargo_debug.sh`, `cargo_release.sh`, `cargo_test.sh`, `cargo_coverage.sh`, `cargo_fmt_clippy.sh` (`CARGO_CLIPPY_ARGS`), … | [`linux/scripts/02-toolchain/rust/`](third_party/ANTfrastructure/linux/scripts/02-toolchain/rust) | inline cargo invocations |
| Linux packaging (tar/deb/AppImage/Flatpak) | `package_archive.sh` | [`linux/scripts/06-packaging/package_archive.sh`](third_party/ANTfrastructure/linux/scripts/06-packaging/package_archive.sh) | bespoke packaging |
| CI job plumbing | `prepare-linux-ci-host`, `run-in-linux-container`, `run-in-windows-container`, `clone-into-short-path`, `cleanup-disk-space`, `assert-docker-disk-space` | [`.github/actions/`](third_party/ANTfrastructure/.github/actions) | hand-written `docker run` blocks |
| Linting workflows locally | `lint-workflows.sh <root>` (pinned, SHA-verified actionlint) | [`linux/scripts/lint-workflows.sh`](third_party/ANTfrastructure/linux/scripts/lint-workflows.sh) | bootstrapping your own |
| Agentic loop | config + runner templates | [`shared/agentic-loop/templates/`](third_party/ANTfrastructure/shared/agentic-loop/templates) | writing one from scratch |
| Bash helpers (logging, retry, SHA'd downloads, parallelism) | `logging.sh`, `downloads.sh`, `parallelism.sh`, … | [`linux/scripts/01-core/`](third_party/ANTfrastructure/linux/scripts/01-core) | new implementations |

Two caveats:

- **Nested module imports are module-private.** `WindowsBuild.Common` importing `WindowsScripts.Shared` does not re-export it to you; import each module you call into directly, or you get a "command not found" the first time that code path runs.
- **Editing the submodule is allowed** (it is the same owner), but it is consumed by other repos. Change it there, push, then move this repo's submodule pointer — do not fork behaviour locally.

Nothing here needs Windows PowerShell 5.1 semantics: every script carries `#requires -Version 7.0` and CI invokes `pwsh`.

## 3. Critical invariant: submodule pins

Builds are only supported against the **recorded submodule gitlink** — the commit CI
builds green. `git submodule update --checkout --recursive` restores it. If a drifted
submodule is what you actually want, move the gitlink **and** fix the fallout in the
same change; do not fork upstream behaviour locally.

**The `.gitmodules` url is `https://`, never `git@github.com:`.** An SSH url works only
for someone with a key loaded, and it fails in the two places that matter most: a hosted
runner, which has none, and a recursive checkout from a superproject — OmniAccelerANT
builds this repo as a submodule, so its clone walks into this `.gitmodules` and inherits
whatever it says. `actions/checkout` installs a `url.<https>.insteadOf` rewrite that
papers over an SSH pin, but a bare `git submodule update` does not, and that is what died
with `Permission denied (publickey)`.

There is one submodule, `third_party/ANTfrastructure`, and every gate in this repo comes
out of it: the images both build lanes run in, the shellcheck/actionlint/gitleaks
binaries the lint lane bootstraps, the packaging and docs drivers, the Windows modules.
A drifted gitlink does not degrade one job — it silently changes every gate, and
`git submodule status` marks it with a `+` that is easy to miss in a wall of CI output.

Guarded by ANTfrastructure's own repo-agnostic Pester suite, run after any pin bump from
[`.github/workflows/submodule-pins.yml`](.github/workflows/submodule-pins.yml) — which
since 2026-09-15 is one `uses:` line onto the hub's reusable
`submodule-pins.yml`, keeping only the `on:` filters that say when the lane runs. The
suite is `third_party/ANTfrastructure/shared/windows/tests/Submodule.Pins.Tests.ps1`; it
asserts for **every** configured submodule that it is checked out, sits at its recorded
commit, and is pinned to a commit still reachable from its remote — so a second submodule
is covered the day it lands, without editing the lane.

The Pester pin (`3.4.0`) and the `windows-2025` runner are the hub's choices now, not
this repo's, and their forty lines of rationale live upstream once
(`third_party/ANTfrastructure/docs/windows-builds.md`). It is still a **separate
workflow from `lint-gates.yml`**, for the reason it always was: Pester 3.4.0 is a
Windows PowerShell-era module never released for PowerShell Core on Linux.

Version couplings with the hub, checked: the toolchain (`RUST_VERSION`) and the two
cargo-tool pins (`CARGO_AUDIT_VERSION`, `CARGO_DENY_VERSION`) are read from
`third_party/ANTfrastructure/linux/scripts/01-core/versions.env` at run time rather than copied, and no CI image
reference is written down here at all — `verify_ci_image_refs.py` check D fails the lint
gate if one is.

## 4. Pitfalls specific to this project

Everything here is false or meaningless in another repo — that is why it is written out
rather than linked.

### The two traps a Windows checkout hits before anything runs

- **A CRLF checkout breaks it before anything runs.** The scripts are executed by bash inside the container; a `\r` makes it fail with `set: pipefail\r: invalid option name`, which names neither the file nor line endings. `.gitattributes` now pins `*.sh` to LF in both repos, but git does not rewrite an existing checkout: `git ls-files -z '*.sh' | xargs -0 rm -f && git checkout -- .`
- **The image's Rust may be older than its own pin.** See "Known gaps" — `latest-cross` shipped Ubuntu's rustc 1.93.1 while `versions.env` pinned 1.97.1, which surfaced as a dependency's MSRV error, not as an image problem. Fixed in ANTfrastructure; check `rustc --version` in the container if a build fails on an MSRV floor.

### The bin was renamed, not the lib — do not undo it

**The pdb collision is fixed — do not undo it by renaming the bin back.** Cargo used to warn that the root **lib** and the CLI **bin**, both named `oxidant`, wrote the same `oxidant.pdb` (it comes from the lib's `cdylib` crate type, not the rlib), and that this *"may become a hard error in the future"* — [rust-lang/cargo#6313](https://github.com/rust-lang/cargo/issues/6313).

The **bin** was renamed, not the lib, and that direction was deliberate: the C++ Vulkan engine imports the lib through Corrosion/cxxbridge (the generated `oxidant_bridge` target in the parent repo's CMake), so `[lib] name` decides DLL/LIB filenames that another repository depends on.

What moved with the bin: `Msix.Binary` and `Msi.OutputName` in `scripts/windows/Build-Windows.config.psd1`, `File Name=` in `wix/main.wxs`, `BINARY_FILE` in the Linux lane (`reusable-linux.yml` today) and `BINARY` in the Windows one (`windows-x64.yml`), the `-Binary` default in `Invoke-AppProfiles.ps1`, and `--bin` in `scripts/linux/run-person-detection.sh`.

**`[lib] name` did change later, on 2026-09-05**, when the repository became OxidANT: `[package] name` and `[lib] name` are both `oxidant` now, so the artefacts are `oxidant.dll` / `liboxidant.so` / `liboxidant.a`. That is exactly the outside-this-repo break the paragraph above warns about, and it was only safe because every consumer was updated in the same commit — OmniAccelerANT's Cargokit wiring, podspecs, `Get-WindowsBuildConfig.ps1` and the committed `frb_generated.dart` loader stem, plus BeschleunigerBallett's Corrosion import. Renaming it again means finding those consumers again — they are listed under [Consumers](#consumers).

The two `BINARY` variables still mean different things. In the **Windows** workflow (`windows-x64.yml`) it is the executable (`kataglyphis_cli`). In the **Linux** one (`reusable-linux.yml`, which both Linux lanes call) it is `oxidant`, and it names *both* the tarball and the file inside it: `package_archive.sh` copies `target/release/$BINARY_FILE` to `$ArchiveDir/$Binary`. So `BINARY_FILE` is the cargo artefact, `BINARY` is what a user ends up invoking.

### Inside the Stevedore Windows container

Because cargo is what makes it bite:

- **Every build write stays container-local** — `CARGO_TARGET_DIR=C:\ct`,
  `CARGO_HOME=C:\ch`. Cargo's create-then-rename is exactly the pattern a bind
  mount rejects, so only a plain artifact copy crosses the mount at the end.
  That copy direction works; do not "simplify" it into a rename or a `docker cp`.
- **`-StageSources`** restores the old robocopy-to-`%LOCALAPPDATA%\Temp` path for
  a host whose Dev Drive filters were never allowed (symptom: `docker run` exits
  immediately with *"Der Dateisystem-Minifilter kann nicht an das
  Entwicklervolume angefügt werden"*). The permanent fix is a host setting — see
  *Dev Drive filter setup* in the performance doc above; `-StageSources` is the
  workaround, not the cure.
- **Mount target must not already exist in the image** — hence `C:\ws-mnt`.
- **Get container plumbing from the modules, never inline**: `Resolve-DockerExe`,
  `Get-ContainerIsolationArgs`, `Remove-BuildContainerSafe`, `Wait-ContainerExit`.
  `Remove-BuildContainerSafe` returns whether the name is actually free — a bare
  `docker rm -f` can return while teardown still holds it — and on `$false` the
  driver switches to a unique name rather than let the next run inherit the held
  container's stale exit code. `Wait-ContainerExit` replaced the driver's
  hand-rolled wait loop (which had no timeout, tested state fail-open, and read
  `docker inspect` through `2>$null`); the driver bounds it at `-TimeoutMinutes
  60`, >10× the per-phase baselines below. The wait's own history and contract:
  the submodule's `docs/windows-container-build-performance.md`, § *Reusable
  implementation*.
- **Run containers named and without `--rm`** so logs and state survive a dropped
  client, and tee important output to the mounted scratch dir. Both are
  load-bearing for `Wait-ContainerExit`: `--rm` deletes the exit code with the
  container. A failed run's container is now kept for `docker logs`; the next run
  (or `docker rm -f`) clears it.
- **Everything here is `pwsh` (PowerShell 7+).** Every script under
  `scripts/windows/` carries `#requires -Version 7.0`, so any surviving "Windows
  PowerShell 5.1" comment is wrong — those scripts would not start under it. Keep
  `$ErrorActionPreference` at `Continue` in the in-container scripts and check
  `$LASTEXITCODE` manually anyway.
- **Still not adopted:** `Test-BuildArtifactsDelivered` (a green build is not
  proof of delivery — it `docker exec`s the container, so it must run before
  teardown) and `Test-ContainerBindMount`. Both would fit
  `Invoke-StevedoreBuild.ps1`; neither could be exercised from the Linux
  verification box.

### `cargo_fmt_clippy.sh`

**The lint step runs the driver now, with a narrower clippy scope.** Both of the reasons this repo hand-rolled `cargo fmt` / `cargo clippy` are gone: the leading `rustup component add rustfmt` (the driver probes first — its header says PROBE, DO NOT ADD), and the hard-coded `--all-features`, which is a `CARGO_CLIPPY_ARGS` knob since the pinned hub. `scripts/linux/ci-container-steps.sh` sets it to `--workspace --locked` — the scope the hand-rolled pair used — because this image cannot build `--all-features` (GTK4/ORT, and uid 1001 cannot install either). Note the other half of that change: positional arguments now reach `cargo fmt` **only**, not both tools. See the CI section.

### Never expand a manifest template with `-replace`

PowerShell treats the replacement side as a substitution template, so a value containing `$&` re-inserts the whole matched token. A description of ``Renderer $& x`` produced `Desc="Renderer __MSIX_DESCRIPTION__amp; x"` — the literal token, shipped into the manifest. `Expand-XmlTemplateTokens` uses an ordinal `[string].Replace` and escapes each value itself.

### Feature combinations and their system dependencies

Default features are empty, so `cargo build` needs nothing. Each optional feature pulls system libraries that must already exist — **the CI container runs as uid 1001 and cannot `apt-get install` them**:

| Feature | Needs | In the CI image? |
| --- | --- | --- |
| `gstreamer` (crates/media) | GStreamer dev files | **Yes** — source-built into `/opt/gstreamer`, on `PKG_CONFIG_PATH`. Do *not* install the distro `libgstreamer*-dev`: the image purges those on purpose. |
| `gui_linux` | GStreamer + wgpu (pure Rust) | **Yes** — no GTK. It pulls the wgpu dependencies but compiles no GUI module of its own: the wgpu GUI (`crates/gui/src/gui_wgpu`) sits behind `gui_windows`. |
| `gui_windows` | The same as `gui_linux` | **Yes** — despite the name it checks on Linux, and it is the only feature that compiles the wgpu GUI there, which is why the `feature-matrix` job carries it. |
| `gui_unix` | `libgtk-4-dev` | **No, by design.** The foreign-arch GTK dev chain pulls target-side Python and breaks cross builds on `python3-minimal`'s postinst. This feature cannot be built against `:latest`. |
| `onnxruntime`, `burn_demos` | Nothing at build time: every ORT feature is `load-dynamic`, and `download-binaries` (pyke's prebuilt ORT, plus `openssl-sys` for its TLS) is banned by the owner rule of 2026-09-23 — `scripts/linux/check-ort-chain-only.sh` gates it in CI. At run time, the image's chain-built ORT, which `crates/inference/src/ort_runtime.rs` finds (`ORT_DYLIB_PATH`, the exe's directory, then the image prefix — never a bare-name load) and refuses unless the file embeds the chain's ORT source path | **Yes** — `/usr/local/lib/onnxruntime-cpu/lib` (Linux), `$env:ONNX_ROOT\bin` (Windows). |

On a plain Ubuntu box (e.g. the WSL recipe above) you *do* need the distro packages, because nothing there provides the source-built stack. That difference is exactly why "install the -dev package" is the wrong instinct when the image is involved.

**Every feature path lints clean** — measured 2026-08-07 on Ubuntu 24.04 with rustc 1.97.1, `cargo clippy --all-targets --locked --features <set> -- -D warnings`:

| Feature set | Result |
| --- | --- |
| default (what CI lints) | clean |
| `gstreamer,gui_linux,onnxruntime,onnx_tract` | clean |
| `gui_unix` | clean |
| `burn_demos` | clean |

Worth stating plainly because none of the non-default rows has *ever* been linted in CI: the Linux lane lints default features (which are empty) and the Windows lane's fmt/clippy silently skip. `crates/media`, `crates/gui` and the ONNX paths are unguarded, not neglected — the `feature-matrix` job exists to keep it that way.

Note the feature names belong to the **root package**. `cargo clippy --workspace --features gstreamer` fails with *"package `kataglyphis_gui` does not have feature `gstreamer`"* because `--workspace` applies the list to every member; drop `--workspace` to scope it to the root.

### Do not let `cargo update` take zune-core to 0.5.2

`zune-core` is held at **0.5.1** in `Cargo.lock` on purpose. 0.5.2 breaks `zune-jpeg` 0.5.15:

```
error: macro expansion ends with an incomplete expression: expected expression
  --> zune-jpeg-0.5.15/src/mcu_prog.rs:463:17
error: could not compile `zune-jpeg` (lib) due to 1 previous error
```

zune-jpeg consumes a macro from zune-core, and 0.5.2 changed it. **There is no forward fix**: 0.5.15 is zune-jpeg's newest release and 0.5.2 is zune-core's, so the two are incompatible at their respective tips. Both arrive transitively (via `image`, into the renderer), so nothing in a `Cargo.toml` pins them — only the lockfile does.

A bare `cargo update` reintroduces it silently, and it only shows up in a **release** build of the full workspace; `cargo test` and `cargo check -p ...` stay green because they never reach that crate. If you run `cargo update`, put it back:

```bash
cargo update -p zune-core --precise 0.5.1
```

Re-check when zune-jpeg publishes past 0.5.15.

### Known gaps

- **No always-on CI lane builds any optional feature.** Both Linux lanes build default features; the Windows lane builds `gui_windows,onnxruntime_directml` only (on every push since 2026-09-24). So `crates/media` and the burn demos have no automated coverage — that is how the GStreamer version skew (since fixed) survived unnoticed. The `feature-matrix` job in `linux-x64.yml` closes this, but it is still opt-in (`[build-features]`), and only once the build image ships the three package groups in the table above.

- **No CI lane has a GPU, so the golden tests never actually run.** `GpuContext::headless_or_skip()` returns `None` and every one of the ~40 headless render tests reports as passed having drawn nothing. This is not theoretical: running them for real (WSL + llvmpipe, 2026-08-07) surfaced a **pre-existing, deterministic rendering bug**:

  ```
  a_non_uniform_instance_scale_shades_like_the_same_node_scale
  crates/webgpu_renderer/tests/skinned_bounds.rs
  node-scale and instance-scale must shade the same, 987 pixels differ  (threshold: 40)
  ```

  Confirmed pre-existing by re-running it against a pristine export of `a618287`: byte-identical 987. Everything else in the suite (~340 tests) passes. The failing path is the instanced normal transform — the generated `src/shaders/forward.wgsl` applies `instance_cofactor_0` to `worldNormal_0`, and the two shading paths disagree where they must agree.

  **Fix it upstream, not here.** The `.wgsl` files in `src/shaders/` are checked-in *generated artifacts*; there is no `.slang` file in this repo, and the code comments reference the C++ engine's `forward.slang` by line number (e.g. `cascades.rs` → `forward.slang:151`). Hand-editing the generated WGSL would desynchronise it from its source.

  Until a CI runner has an adapter, a software one makes these tests real: install `mesa-vulkan-drivers` and set `KATAGLYPHIS_REQUIRE_GPU=1` so a missing adapter fails loudly instead of skipping silently.

### Verifying locally on a Windows box (no MSVC required)

This repo's dev machines commonly lack the MSVC "C++ build tools" workload. Without it **nothing links**, and Git Bash makes the failure baffling: `/usr/bin/link.exe` is coreutils' `link`, which shadows the MSVC linker on PATH and dies with `link: missing operand after '\377\376'` (it is being handed rustc's UTF-16 response file). That is an environment fault, never a code fault.

The fastest real fix is to verify in WSL against the CI's own target platform:

```bash
# once, as root inside the distro
apt-get install -y build-essential pkg-config curl git libssl-dev \
    libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgtk-4-dev
# optional: a software Vulkan adapter so the headless golden tests actually run
apt-get install -y mesa-vulkan-drivers vulkan-tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/ri.sh
# The toolchain is the fleet's, read from the submodule rather than typed here - it
# was pinned to a literal 1.97.1 until versions.env moved to 1.98.1 underneath it.
RUST_VERSION="$(. third_party/ANTfrastructure/linux/scripts/01-core/versions.env; echo "$RUST_VERSION")"
sh /tmp/ri.sh -y --profile minimal --default-toolchain "$RUST_VERSION" -c rustfmt -c clippy
```

Then, from the repo root, with `CARGO_TARGET_DIR` pointed at a **Linux-native** path (never the 9p-mounted Windows tree, which is glacial and already holds MSVC artifacts):

```bash
export CARGO_TARGET_DIR=/root/kt
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
KATAGLYPHIS_REQUIRE_GPU=1 cargo test --workspace --locked
```

`KATAGLYPHIS_REQUIRE_GPU` is the important one: without it `GpuContext::headless_or_skip()` silently returns `None` and the whole golden-test suite "passes" having rendered nothing. Set it and a missing adapter becomes a panic, so green *proves* the tests ran.

## 5. Build, run, test

```bash
cargo build --workspace --locked                      # dev/debug
cargo build --workspace --locked --profile profile    # custom: release + debuginfo
cargo build --workspace --locked --release            # fat LTO, codegen-units 1, panic=abort, stripped
cargo test  --workspace --locked                      # unit + integration + proptest fuzz + doc tests
```

Run the lint gates before pushing. CI's formatting-and-clippy step is ANTfrastructure's `cargo_fmt_clippy.sh` with `CARGO_CLIPPY_ARGS='--workspace --locked'`, which is the same pair of hard failures written below — reproduce the step itself with `bash scripts/linux/ci-container-steps.sh fmt-clippy` inside the image. The shell/workflow/secret/config gates are `bash scripts/linux/run-lint-gates.sh` (see [Continuous integration](#continuous-integration)):

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --workspace --locked -- -D warnings
```

Default features are empty — GUI and ONNX code only compiles with explicit `--features` (see README "Run"). "Fuzz" testing = proptest in `tests/fuzz_test.rs`; there is no cargo-fuzz/libFuzzer target.

### The cat producer on a Raspberry Pi 5

`scripts/linux/cat-stream/run-producer-pi.sh` builds and runs
`crates/cat_webrtc` from the family CI image against a Pi 5 CSI camera. It moved
here from OmniAccelerANT on 2026-09-15 under decision D12, because the crate it
drives is this repo's; OmniAccelerANT keeps a pointer and still owns the web half
(`scripts/linux/cat-stream/serve.sh` over there).

```bash
scripts/linux/cat-stream/run-producer-pi.sh --build       # first run: cargo build in the image
scripts/linux/cat-stream/run-producer-pi.sh               # start the producer
scripts/linux/cat-stream/run-producer-pi.sh --libs-only   # refresh build/cat-stream/hostlibs
```

Two things it deliberately does **not** spell out, because ANTfrastructure owns
both and a copy here goes stale:

- **the image reference** — it asks
  `third_party/ANTfrastructure/linux/scripts/ci-image-ref.sh`, which reads the
  fleet's `versions.env`. A literal `ghcr.io/…:latest` in a tracked `*.sh`
  is what the lint lane's CI-image-ref gate fails on.
- **`LD_LIBRARY_PATH`** — the container prologue sources the image's own
  `/opt/scripts/03-media/final/media-env.sh` (the same file the Dockerfiles
  source) and prepends only `/hostlibs` through
  `/opt/scripts/core/path-helpers.sh`'s `_path_prepend_unique`. The retyped list
  it replaced had `/opt/gcc-16.2.0` hard-coded, so a GCC bump upstream would have
  silently dropped the C++ runtime out of the search path.

Only `/hostlibs` (the host's Raspberry Pi OS libcamera closure, which must win
over the image's older upstream copy) and the host multiarch directory are this
script's own contribution to the loader path.

### Build & test in the Stevedore Windows container

Driver: `scripts\windows\container\Invoke-StevedoreBuild.ps1` (add `-Test` to also run the test suite; `-TestOnly` to skip building). It **bind-mounts this repository straight into the container** as `C:\ws-mnt`, runs the in-container scripts (`Build-RustAll.ps1`, `Test-RustAll.ps1`) in the family Windows CI image, and the artifacts land directly in `target\container\<profile>`, mirrored to the gitignored root `debug\`, `profile\`, `release\`.

**No image reference is written in this repository, and that includes this
file.** The driver's `-Image` parameter defaults to empty and is filled in by
ANTfrastructure's `Get-CiImageReference -Windows`, which composes
`IMAGE_REGISTRY_PREFIX` + `CI_IMAGE_WINDOWS_TAG` from the submodule's
`linux/scripts/01-core/versions.env` — the fleet's single owner of both CI
refs, so a tag bump lands in one file in one repo and arrives here with no
edit. Pass `-Image` to override for a one-off. Prose is not exempt: a
reference typed into a table or a README goes stale exactly like one typed into
code, and `verify_ci_image_refs.py` check D fails a build on a copy in any
tracked `*.sh` / `*.ps1` / `*.psm1` or workflow YAML.

Mounting the repo — not a copy of it — is the default, ReFS Dev Drive or not.
It also means `third_party/` is present inside the container, so anything
importing ANTfrastructure modules (e.g. `Build-Windows.ps1`) works without special
staging.

**The host-side mechanics are upstream's, not this repo's.** Why writes through
a bind mount fail while reads succeed, how to allow the Dev Drive filters, what
`--isolation process` does to the CPU count, the wcifs teardown lock, the
transient hcsshim client-pipe drops, and why the Windows lane is Stevedore's
`docker.exe` rather than nerdctl: all in
[`docs/windows-builds.md`](third_party/ANTfrastructure/docs/windows-builds.md)
and
[`docs/windows-container-build-performance.md`](third_party/ANTfrastructure/docs/windows-container-build-performance.md).
Read those before changing the driver. **Do not copy their commands back into
this file** — the last copy of the `fsutil devdrv` line that lived here was
malformed and stayed that way through several edits.

### Continuous integration

Six workflow files: five triggered, one reusable. The three build lanes — one file per platform + arch — run inside ANTfrastructure images rather than on the runner; the gate lanes pull no image at all, and **none of the three gate jobs is a copied job any more** — all three are one `uses:` onto ANTfrastructure, two onto a reusable workflow and one onto a composite action:

| Lane (display name) | Workflow | Runs when | Image and runner |
| --- | --- | --- | --- |
| Lint gates | `lint-gates.yml` (job `lint-gates`) | every push/PR to `main`/`develop` | none — the hub's reusable `lint-gates.yml` with `ratchets: true`; the same aggregator `bash scripts/linux/run-lint-gates.sh` runs locally |
| Lint gates | `lint-gates.yml` (job `powershell-lint`) | same | none — the hub's reusable `python-ci-windows.yml` with `build-python-package: false`, `lint-powershell: true`, `lint-path: scripts`, and no `secrets:` block; `Invoke-Lint.ps1 -Path scripts -FailOnAnalyzer` on `windows-2025` |
| Lint gates | `lint-gates.yml` (job `generated-artifacts`) | same | none — `scripts/windows/tests/` under Pester 3.4.0 on `windows-2025` |
| Submodule pins | `submodule-pins.yml` | push/PR to `main`/`develop` touching `.gitmodules`, `third_party/**` or itself | none — the hub's reusable `submodule-pins.yml` (`Submodule.Pins.Tests.ps1`, Pester 3.4.0, `windows-2025`) |
| Linux x64 · build + test | `linux-x64.yml` → `reusable-linux.yml` | **every** push/PR to `main`/`develop`, and `workflow_dispatch` | family Linux CI image, inherited; `ubuntu-26.04`. The only lane that builds and publishes the docs |
| Linux arm64 · build + test | `linux-arm64.yml` → `reusable-linux.yml` | same | same image (a multi-arch index); native `ubuntu-26.04-arm`, no QEMU |
| Windows x64 · build + test | `windows-x64.yml` | same | family Windows CI image, inherited; `windows-2025` |
| Linux x64 · build + test | `linux-x64.yml` (job `feature-matrix`) | opt-in: `[build-features]` in the HEAD commit message, or `workflow_dispatch` | family Linux CI image; `ubuntu-26.04` |

**Every platform lane runs on every push and PR, since 2026-09-24** (owner request).
None of the three build jobs carries an `if:`, and none may be added back. Until that
date the arm64 row needed `[build-arm]` and the Windows lane `[build-win]` in the HEAD
commit message, so both reported `skipped` on almost every push — which a badge
renders the same as a pass. ANTfrastructure's `docs/ci-build-triggers.md` still
describes those markers for the family; nothing here reads them any more. The feature
check is the one opt-in job left, and a skipped job inside a green Linux x64 run does
not skip that workflow.

**The names follow the family convention of the same date:** kebab-case files, one per
platform + arch, display names `<Platform> <Arch> · <what>` (U+00B7 middle dot).
Steps both architectures share live once, in `reusable-linux.yml` ("Linux · reusable
build", `workflow_call`); `linux-x64.yml` and `linux-arm64.yml` only say when they run
and which runner, container platform and artifact suffix they want. The gate lanes
kept their files and took plain names, "Lint gates" and "Submodule pins".
Concurrency is per workflow: each triggered build workflow groups on
`${{ github.workflow }}-${{ github.ref }}` and cancels a superseded run except on the
default branch. `reusable-linux.yml` declares no group, because inside a called
workflow `github.workflow` is the caller's name, and GitHub cancels a callee whose
group repeats its caller's as a deadlock.

"Inherited" is literal: **no build workflow names an image.** Both of the old ones used to open
with a `CONTAINER_IMAGE:` env entry holding the full reference and hand it to
every container step; that was a copy of ANTfrastructure's `versions.env` value
that a fleet-wide tag bump would leave behind. Every step now omits the `image:`
input and takes the container actions' default, which
`verify_ci_image_refs.py` grades against `versions.env` on ANTfrastructure's own
build (check A) — and check D fails this repo's lint gate if the reference is
re-typed into a workflow, a script, or a comment.

**PowerShell is graded too, and with the analyzer enforcing.** The job is a `uses:` since 2026-09-15: hub `604294e2` gave `python-ci-windows.yml` a `build-python-package` input (boolean, default true) gating its container build and made `GHCR_PAT` optional, so a Rust crate can take the family's PowerShell lint without buying a Python package build. `Invoke-Lint.ps1` makes two passes — a parse + AST-trap gate that is always fatal, and PSScriptAnalyzer. The sibling consumer this job is modelled on leaves the analyzer advisory because it has untriaged findings; this tree has none (measured 2026-09-15: 0 errors, 0 warnings over all 9 files), so `-FailOnAnalyzer` is passed and the ratchet is set at the number this repo actually has. `-Path scripts` is walked recursively, and the hub's `PSScriptAnalyzerSettings.psd1` is resolved against the *script's* own location — the ruleset is consumed by reference and nothing is copied in here.

**Nothing generated may be tracked.** `scripts/windows/tests/Repo.GeneratedArtifacts.Tests.ps1` runs ANTfrastructure's `Get-TrackedIgnoredFile` and `Get-TrackedGeneratedArtifact` over this root; only the root and the list of generated-output pathspecs are local. A `.gitignore` rule stops a file from being *added* and does nothing once a path is in the index, which is why `git rm --cached` has already been needed twice here (build logs under `logs\windows\`, and the flatpak repo). Note the pathspec form: `'**/__pycache__/*'` matches, `'**/__pycache__/'` matches **nothing** and would grade zero paths while reporting clean.

**The lint lane runs with `--ratchets` on** since 2026-09-15. On top of the six always-on gates that adds the docs cross-reference gate plus eight measurement gates (code size, complexity, dead functions, comment size, stdout returns, masked declarations, trailing conditionals, and a shellcheck *warning* ratchet), each graded against a freeze file at the repo root. Three of them carry rows — `comment-size.allow`, `code-complexity.allow`, `dead-functions.allow` — seeded from the first run; the rest are absent, which the gates read as a zero baseline. **The contract is two-way**: a new offender fails, and so does an entry that is no longer over the limit, so fixing one of these means deleting or updating its row in the same change. The docs gate has no freeze file at all and never will — a `docs/…md` pointer in code either resolves from the repo root (or, when it starts `../`, from the file) or it is a finding.

Facts that cost real debugging time:

- **The arm64 lane runs natively on `ubuntu-26.04-arm`, on every push since 2026-09-24.** It was opt-in via `[build-arm]` for runner minutes before that, never because it could not pass: `:latest` (then `:latest-cross`) has been a multi-arch index since 2026-09-04.
- **Linux artifacts are named by `VERSION`, not by `github.ref_name`.** On a pull request the ref name is `<number>/merge`, `upload-artifact` refuses a `/` in a name, and every PR run of the Linux lane went red at *Upload all artifacts* with the build, tests and package green (run 35752031200, 2026-09-22). Fixed on 2026-09-24, when the lane started running on every PR on both architectures.
- **Every container step of both Linux lanes is one named step of one script**, `scripts/linux/ci-container-steps.sh` (`debug`, `security`, `fmt-clippy`, `test`, `coverage`, `bench`, `release`, `docs`). Each step used to inline its own `bash -lc 'set -e; git config --global --add safe.directory /workspace; bash third_party/.../cargo_<x>.sh'` — the same prologue eight times. Reproduce any step by hand with `bash scripts/linux/ci-container-steps.sh <step>` inside the image; `reusable-linux.yml` runs exactly that line.
- **`cargo fmt`/`cargo clippy` run through ANTfrastructure's `cargo_fmt_clippy.sh`** like every other step — `fmt-clippy` was the one case that did not delegate, and stopped being one on 2026-09-15. Two things had to change upstream first, and both did: the driver's old first line `rustup component add rustfmt` exited 127 on an image without rustup (it probes first now — header: PROBE, DO NOT ADD), and `--all-features` was hard-coded at its line 40, which this image cannot build (GTK4/ORT, see the last bullet). The scope is `CARGO_CLIPPY_ARGS`, set to `--workspace --locked` in `scripts/linux/ci-container-steps.sh`. That exit 127 was masked by `continue-on-error: true` for months and let an entire crate reach the default branch unformatted and with 12 clippy errors — the step gates now.
- **The docs publish follows the repository's own default branch**, not a typed `refs/heads/main`. It asks for `format('refs/heads/{0}', github.event.repository.default_branch)`, and so does `cancel-in-progress`. The literal was wrong for as long as `main` was abandoned and `develop` carried every commit: the comment said "only publish from the default branch" while the condition matched a branch nobody pushed to, so <https://rust.jonasheinle.de> was never republished at all. A red **Security checks** step has the same effect for a different reason — the publish is the last step of that job.
- **The container runs as uid 1001, not root.** `apt-get` fails with `Permission denied`, so a workflow step cannot install system packages — whatever the image lacks, it lacks. And `CARGO_HOME=/usr/local/cargo` is root-owned, so every writing cargo step needs `-e CARGO_HOME=/tmp/cargo-home`.
- **The lint gate runs default features on purpose.** `--all-features` would need GTK4 headers (`gui_unix`), which the image has not got and uid 1001 cannot install.
- **The Windows lane pins its cargo tools.** `scripts/windows/Build-Windows.ps1` installs cargo-audit and cargo-deny with `--version`, read from `$env:CARGO_AUDIT_VERSION` / `$env:CARGO_DENY_VERSION` (baked into the image) and falling back to the submodule's `versions.env` through `ConvertFrom-VersionsEnv`; unresolvable **throws**. Unpinned, `cargo install` takes whatever crates.io serves that minute, so a new advisory-db schema turns the lane red with no commit to bisect. There is no try/catch around the install any more either — a swallowed failure left both gates running whatever was on PATH.
- **`cargo_security_checks.sh` is a gating step, not an advisory one.** The *Security checks (cargo audit + cargo deny)* step in `reusable-linux.yml` (so in both Linux lanes) runs ANTfrastructure's `linux/scripts/02-toolchain/rust/cargo_security_checks.sh`, and a finding fails the lane — which also means the docs publish at the end of the x64 lane never runs while it is red. It reads **two** ignore lists and they must stay byte-identical in content: `.cargo/audit.toml` (cargo-audit) and `deny.toml` `[advisories].ignore` (cargo-deny). An id added to one and not the other buys nothing — the other tool still reports it. Prefer an upgrade over an ignore: `chacha20` 0.10.1 (yanked) and `stable-vec` 0.4.2 (unsound) both had one, so neither is on either list.

Lint the workflows locally with the submodule's pinned, SHA-verified actionlint (works from Git Bash on Windows):

```bash
bash third_party/ANTfrastructure/linux/scripts/lint-workflows.sh .
```

The trailing `.` is load-bearing: without it the script lints ANTfrastructure's own workflows instead of this repo's, and reports green either way.

### Dependency upgrades

Renovate, run as a **local CLI**. Both lanes above build inside images the
`third_party/ANTfrastructure` pin decides, so that gitlink drifting is a silent
change to every gate — and nothing watched it before this wrapper existed.

```bash
bash scripts/linux/renovate-local.sh                    # report (default: git-submodules)
bash scripts/linux/renovate-local.sh --managers cargo   # the workspace crates
bash scripts/linux/renovate-local.sh --apply --dry-run  # the plan
bash scripts/linux/renovate-local.sh --apply            # move the gitlink
```

Run it from **WSL** — the wrapper bootstraps a pinned Node and there is none on
the Windows side. Nothing runs it for you: **the Renovate GitHub App is
installed on no repo in this family and will not be**, so this CLI is the only
thing that ever reads `.github/renovate.json`. No workflow calls it and it
blocks no commit.

`--apply` moves **gitlinks only**, and only for submodules that declare a
`branch =`. Here that is the one entry in `.gitmodules` — measured on
2026-09-09, the default report was a single row, `third_party/ANTfrastructure
fb7d673dd383 → 6ad5d8802e78`, in about four seconds. Cargo is **report-only**:
`--managers cargo` returned 21 rows the same day, and exactly one of them
(`flutter_rust_bridge =2.12.0 → =2.13.0`) is a manifest edit. The rest print the
same string twice (`wgpu 30 30`) because the declared range already covers the
new release — a `cargo update`, not a `Cargo.toml` change. Editing manifests is
still `cargo upgrade` and Dependabot's PRs; `.github/dependabot.yml` stays.

Managers that reach `api.github.com` answer short without a token and say so.
The variable that fixes that under `--platform=local` is `GITHUB_COM_TOKEN`, not
`RENOVATE_TOKEN`: `GITHUB_COM_TOKEN="$(gh auth token)" bash scripts/linux/renovate-local.sh --managers cargo`.

The script header covers the rest; the family rationale is
[`third_party/ANTfrastructure/docs/dependency-updates.md`](third_party/ANTfrastructure/docs/dependency-updates.md).

## 6. Docs owned by this repo

`AGENTS.md` (this file), `README.md`, `BACKLOG.md`, `CHANGELOG.md`,
`crates/webgpu_renderer/README.md` and the renderer's design documents in
`crates/webgpu_renderer/docs/`. Rustdoc is published from the default branch to
<https://rust.jonasheinle.de> by the Linux lane's last step. Update the docs in the same
change as the behaviour they describe.

### Where the renderer's design documents live

**The move happened on 2026-09-15** (decision D6): this repo owns the renderer, code
and documentation. Three pages left `BeschleunigerBallett/docs/` for
`crates/webgpu_renderer/docs/`; BeschleunigerBallett kept a pointer file at each old
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

**The two halves are referenced differently, and the difference is the point.** The
three that live here are `docs/<name>.md` *relative to the crate* — in
`crates/webgpu_renderer/README.md`, in `src/lib.rs` and twice in
`src/render/bounds.rs`. That path now resolves in every checkout, which is what the
move bought: this repo is built standalone, from OmniAccelerANT and from
BeschleunigerBallett (see [Consumers](#consumers)), and before the move a relative
`docs/` prefix meant *the superproject's* root and pointed at nothing in two of the
three. The four that stayed are still absolute URLs
(`https://github.com/Kataglyphis/BeschleunigerBallett/blob/develop/docs/<name>.md`)
for that same reason, and must stay absolute. `bounds.rs` calls
`renderer-bounds-invariant.md` the checklist for not repeating eight identical bugs,
so a dead link there costs more than tidiness.

### Large tracked files

Two files dominate the size of a clone. Both are tracked deliberately; neither is in
git-lfs, and **the history is not being rewritten** (decision D4 — no `filter-repo`,
no `filter-branch`, no BFG). A rewrite would change every commit id in a repository
that two other repositories pin by sha, for a saving nobody has asked for.

| File | Size | Why it is tracked |
| --- | --- | --- |
| `resources/models/yolov10m.onnx` | ~59 MiB | The YOLOv10m weights. `crates/inference`'s default model and `crates/cat_webrtc`'s `--model` default; the burn demos' `onnx-yolov10` run uses it too. Fetching it at build time would put a network call in front of every build and every offline container run. |
| `images/Rust.gif` | ~7 MiB | The README hero image, rendered by GitHub. |

Everything else tracked is small: the renderer's glTF/GLB/KTX2 test assets are a few
hundred kilobytes each, and they stay that way on purpose.

**Nothing new joins them.** `.gitignore` carries a *Large binaries* block that
excludes model weights, video, packaging artefacts, archives and signing material,
with a negation for each of the two files above rather than a narrow pattern — so the
rule still bites if one of them is moved or renamed. If a third large file really is
necessary, add it to the table above in the same commit that force-adds it.

`logs/windows/` used to hold 15 committed Windows build logs (~2.4 MiB) that
`.gitignore`'s own `logs/**/*` rule already covered; they are untracked now and the
files stay on disk. `Build-Windows.config.psd1` still writes there.

### Conventions

- Version pins/single sources of truth follow the ANTfrastructure ecosystem; don't duplicate what the submodule documents — link to it.
- Never commit build outputs: `/target`, root `/debug`, `/profile`, `/release` are gitignored, and so is `logs/`.
