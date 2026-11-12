# Packages

The existing CPU packages carry the GStreamer libraries and the GUI's plugins,
the chain-built ONNX Runtime, the model, and on Linux the image's C and C++ runtime. On
Windows the MSIX and the MSI are steps of `Build-Windows.ps1`, beside the portable bundle in
`dist\windows-<x64|arm64>\` ([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration));
on Linux `scripts/linux/package-linux.sh` packs one bundle four ways
([§ Linux](#linux-tar-deb-appimage-flatpak)).

Linux GPU packaging is experimental and is not part of the release workflow. The
`Linux x64 · AMD GPU package candidate` workflow runs on request and builds a
candidate requiring host ROCm. Its optional `verify-gpu` job runs packaged inference
on a self-hosted `gpu-amd-rocm` runner and requires profiled operators on the provider `MIGraphX`,
the name AMD's plugin module is registered under.
The manually dispatched or reusable `Linux NVIDIA GPU package candidates` workflow uses
the registry NVIDIA image. Its `architecture` input defaults to `both`; select `x64`
or `arm64` to build and check only that architecture. With `both`, both builds and
checks must succeed. The `verify-gpu` option requires a self-hosted runner labelled
`gpu-nvidia-cuda` for each selected architecture; each must profile a CUDA model
operator. The hardware check profiles the finished tarball, extracted DEB payload, and
AppImage separately; it invokes the AppImage with extract-and-run, without FUSE. A GPU
runner needs `dpkg-deb` and temporary space for those payloads as well as the host GPU
runtime. The clean-host job also exercises the installed DEB command. Uploaded artifacts
remain labelled candidates even when profiling is skipped.
Linux GPU candidate uploads include the run attempt and cannot replace an earlier upload.
Their clean-host and GPU checks request the current attempt's candidate. A downstream-only
rerun cannot reuse an earlier build; rerun the build as well. Attempt-scoped names do not
independently prove GPU execution or make the candidate a release asset.
For multiple calls to the same GPU workflow in one caller run, provide a distinct
`invocation-scope` for each call. It defaults to `default` for existing callers;
other values must start with a lowercase letter and contain at most 32 lowercase ASCII
letters, digits or hyphens. The validated scope isolates artifact and receipt names.
Calls using the same scope still address the same outputs and must not overlap.
For Linux AMD x64 and NVIDIA x64/arm64, `promote-verified` currently fails before the
build: executing GPU operators does not establish that their detector output agrees with
CPU inference. Promotion remains blocked until repeated detector output parity is
verified on hardware. The existing `verify-gpu` profiling and unverified candidate paths
remain available.

The parity diagnostic runs the checked-in two-cats JPEG through the packaged detector in
separate CPU and GPU processes for each package format. It requires two spatially distinct
cats in the CPU result and matches every detection above 0.25 by class, with box IoU at
least 0.90 and score difference at most 0.05. It runs the GPU detector twice in the same
session, repeats the CPU baseline in its own session, compares every result pair and
requires model operators on the requested provider in each run. The CPU-only semantic
oracle is applied to the original CPU baseline; pairwise comparisons retain the same
matching tolerances. These tolerances are provisional until tested on the intended hardware; a
successful synthetic test alone does not unblock promotion.

The prepared promotion path binds clean-host and GPU checks to packages downloaded by
immutable artifact ID, then verifies their receipts and raw archive against the current
run attempt. It rejects missing checks, changed package bytes and downstream-only reruns
from an older attempt. NVIDIA uses separate producers for each architecture. This path
does not publish a release and requires trusted runners and compatible hub APIs.
`package-linux.sh --ort-variant nvidia` selects the chain GPU ORT and its CUDA provider.
`--ort-variant amd` selects the chain CPU ORT and AMD's plugin module `libmigraphx-ep.so` from
the image's `$ORT_AMDGPU_EP_ROOT/lib`; the hub's `stage_ort_runtime` puts the module in the
bundle's `lib/`, beside the ONNX Runtime library, where the detector looks for it
([running.md](running.md)). The module stays byte for byte the image's, and the image's plugin
tree of licence texts and its `provenance.txt` travel as `licenses/onnxruntime-ep-amdgpu/`: no
wheel and no hub helper carries them, so `scripts/linux/lib/ort-runtime-bundle.sh` stages them and
`verify-gpu-packages.sh` compares them with the image's in every format. An AMD package holds no
`libonnxruntime_providers_*` file; the check refuses a tar or deb that does. The variants append
`+gpu-nvidia` or `+gpu-amd` to the version.
The DEBs all use the `oxidant` package identity and install the `oxidant` command;
installing a different variant replaces the existing one rather than installing both.
Finished GPU DEBs are checked for that embedded package identity, the requested target
architecture and the vendor suffix in their version, independently of the filename.
Linux GPU builds target NVIDIA on x64 and arm64, and AMD on x64; arm64 ROCm is skipped.
Pass `--package-types tar,deb,appimage`: Flatpak is excluded because these builds
require vendor libraries from the host. The launcher defaults to `cuda` or `migraphx`
and an explicit `KATAGLYPHIS_ORT_DEVICE` overrides it. The host must install a
compatible GPU driver and CUDA/cuDNN or ROCm/MIGraphX runtime, including its complete
transitive libraries and dynamic-loader configuration. These packages do not bundle
those vendor runtimes. Missing GPU support is an inference error, not a silent switch
to tract; ORT may execute individual unsupported operators on CPU.

No AMD package has been built with the plugin module: the build needs the amd64 `:latest-rocm`
image, and the package checks have run on fixtures only. The earlier local AMD x64 build, which
passed clean Ubuntu installation, CPU inference using the bundled ORT for the DEB, tarball and
AppImage, and a missing-ROCm failure check for the installed DEB, carried the in-tree MIGraphX
provider the image no longer has.
Both GPU workflows now require each finished DEB, tarball and AppImage to select its
vendor GPU by default and report a missing host runtime on the clean-host runner;
that expanded check has not yet run in the hosted workflow. The AMD build job also runs the
packaged launcher in the image, where no card answers: `KATAGLYPHIS_ORT_DEVICE=migraphx` must
fail with `ProviderUnavailable: MIGraphX failed to register` naming the packaged module, and
`auto` must run on the CPU.
Actual GPU inference and the hosted workflow remain unverified. The workflow builds on
the image's GTK 4 and installs no package in the build container. The ROCm image's
74.75 GB local size makes hosted runner disk space a concern, even after the shared
full cleanup.

To verify operator placement on a GPU host, run the packaged command with
`onnx-check --profile /tmp/oxidant-profile` and the desired `KATAGLYPHIS_ORT_DEVICE`.
Then run `python3 scripts/verify-ort-gpu-profile.py <generated-json> --device migraphx`
from this checkout (use `cuda` for NVIDIA). A successful check proves at least one
model operator used that provider, not that every operator did. For `migraphx` the provider is
`MIGraphX`. The parity diagnostic above checks each run with the hub's `verify_ort_profile.py`,
which expects the same name since hub 366b8259; no profile from a card has been checked with it.

The pinned ANTfrastructure wheel verifier rejects the NVIDIA provider in the current image
because it exceeds its 256 MiB input limit, so that package currently stops at notice
verification. A local NVIDIA x64 build with the proposed shared verifier and ELF
closure fixes overlaid on the recorded pin passed package license checks, clean Ubuntu
CPU inference in all three formats, and a missing-CUDA failure check for the DEB. The
overlays are not part of the pin; NVIDIA packaging remains unavailable through the
recorded tooling until they are committed and the pin is updated. The NVIDIA workflow
preflights the exact image wheel and shared finished-AppImage verifier before compiling,
and checks the finished tar, DEB and AppImage payloads before upload. CPU packaging does
not take this path. An emulated local NVIDIA arm64 build with the same helper overlay produced a verified tar; the DEB and AppImage were recovered separately after packaging failed under PRoot. All three ran CPU inference in both glibc modes under QEMU. This is not a successful combined workflow run, a clean-host test, or proof of GPU execution.

Linux bundles include the GTK4 library closure and `gtk4paintablesink` from the
image's actual plugin location, which can differ from GStreamer's pkg-config directory.
The existing bundled-element check requires that sink to load from the package itself.

Each format includes the exact project `LICENSE`; the shared staging helpers reject a
missing or empty source. This check does not prove coverage of every native dependency's
license or corresponding-source obligations.
The Linux tar, DEB, AppImage and Flatpak, the Windows bundle, MSIX and MSI are read back by the shared
license verifier. MSI inspection requires 7-Zip and resolves each license's file identifier from
its source path in the generated WiX fragment. AppImage verification extracts only the license on a matching native host;
cross-architecture verification fails rather than skipping the check.

Linux packaging also matches every bundled `libonnxruntime*.so*` against one
wheel, directly or after target-compatible stripping, and includes that wheel's
original `LICENSE` and `ThirdPartyNotices.txt` in each finished format. Set
`ORT_CHAIN_WHEEL_DIR` to override `/opt/onnxruntime-wheels` and `STRIP` to select
the target's strip tool. Missing, changed or ambiguous matches fail the build;
other native dependency obligations still require their own provenance checks.

Every format also carries `licenses/cargo/`: the original license files of every crate in
`kataglyphis_cli`'s normal dependency graph for that package's target, staged by
ANTfrastructure's `stage-cargo-licenses.py` with a `manifest.json` of names, versions,
sources and SHA-256s. A crate whose archive ships no license file, and every compound `AND`
expression, is resolved in `packaging/cargo-license-policy.json`: each entry names an
original text vendored under `packaging/licenses/cargo/<owner>-<repo>-<commit>/` (fetched
from the commit in the crate's `.cargo_vcs_info.json`) or a file inside the crate, with its
SHA-256 and source URL. A crate without a reviewed text fails the package step, so a
dependency bump that adds one needs a policy entry first; `scripts/linux/tests/test-cargo-license-policy.sh`
stages all four packaged targets and fails on a missing, changed or unreferenced text.
The same policy covers `kataglyphis_cxx_bridge`, whose graph BeschleunigerBallett links and
stages on its own packaged targets, riscv64 included; the test stages that root too.
Linux tar, DEB and Flatpak read back every staged crate file and the model's `LICENSE` and
`NOTICE`; the AppImage, the Windows bundle and MSIX check the crate manifest, the CLI's own
entry and the model license (MSIX part names percent-encode a crate directory's `@`, so
that pattern stops before it); the MSI resolves each through the generated WiX fragment,
because the crate trees repeat the `LICENSE` basename. A Windows payload that ships the
chain `onnxruntime.dll` also carries the matching wheel's `LICENSE` and
`ThirdPartyNotices.txt` (`Copy-OrtWheelLicenses`), checked in every format. The bundled
native libraries (GStreamer, GLib, GTK, FFmpeg, the C and C++ runtimes) are not inventoried
yet.

The MSIX uses only `images/logo.png`. The shared MSIX stager rejects a missing,
malformed or fully transparent image instead of replacing it with another logo.
The MSI's installed `License.rtf` is generated from the current project `LICENSE`
by ANTfrastructure's CMake encoder; its existing installer flow is unchanged.

## Windows GPU portable candidates

After a Windows x64 CPU bundle has been built in the family image, create a separate
portable candidate. GPU candidates are x64 only; Windows ARM64 CPU packages
remain available. The destination must not already exist:

```powershell
pwsh -File scripts/windows/New-GpuCandidate.ps1 `
  -SourceBundle dist/windows-x64/bundle -Destination dist/windows-x64-gpu/bundle `
  -Provider cuda # or directml
```

The script requires the chain ORT, model and wheel notices; it checks ORT provenance,
copies the bundle, and matches the core and bridge to one wheel. CUDA additionally
stages and matches the CUDA provider from that wheel; DirectML requires DirectML.dll.
The source bundle is left unchanged. The GPU APIs in ANTfrastructure are required.

The manually dispatched or reusable `windows-gpu-candidates.yml` uses the registry's NVIDIA CUDA and AMD
DirectML x64 GPU images to build separate unverified portable ZIPs. It checks the pinned
packaging APIs before pulling, checks the image inputs, verifies the finished ZIP,
and runs CPU inference and the bundled camera-pipeline check from the extracted ZIP.
The current hub pin lacks the new
API and intentionally stops the lane before pulling. The GPU image's DirectML DLL
provenance and redistribution terms remain to be checked (ANTfrastructure BACKLOG
CON180). Neither ZIP is an installer or a verified GPU release.

Run `oxidant-cuda.cmd gui` or `oxidant-directml.cmd gui` from the matching candidate;
either launcher also accepts `onnx-check`. The launchers request their named GPU by
default; `KATAGLYPHIS_ORT_DEVICE=cpu` selects CPU inference. CUDA requires the matching
NVIDIA driver and CUDA/cuDNN runtimes on the host; DirectML requires a compatible
DirectX 12 device and drivers. GPU execution is not established by packaging alone.
These candidates are portable only; they do not produce MSI or MSIX installers and
are not part of a release workflow.
Uploads include the run attempt and cannot overwrite an earlier candidate. A rerun of
only the hardware-profile job will not find an earlier attempt's ZIP; rerun the
candidate job as well. Attempt-scoped names do not themselves prove GPU execution.

To profile a candidate on hardware, dispatch with `verify-gpu` and provide Windows x64
self-hosted runners labelled `gpu-nvidia-cuda` and `gpu-amd-directml`. Set
`gpu-vendor` to `nvidia` or `amd` to run one vendor independently; it defaults to `all`.
With `all`, each available candidate can reach its own profile job even when the other
candidate fails; a missing selected artifact still fails its own check. Set
`amd-device-index` to the AMD adapter's DXGI index (default `0`); the job checks that
the same index is a physical AMD GPU before requesting it from DirectML. Each job
requires model operators in its matching ONNX Runtime provider. Its opt-in ZIP check
also compares the locked two-cats fixture on CPU and in two GPU processes, repeating
inference within each session and requiring requested-provider operators in every run.
The bundled model, license and notice must match the model lock. These checks leave
the candidate unverified and do not establish parity for MSI or MSIX packages. The
pinned hub must provide the shared locked-file, parity and per-run profile verifiers.

## MSIX

Needs the Windows SDK (`makeappx`, `signtool`), located by ANTfrastructure's
`Resolve-WindowsSdkToolPath`, which honours VsDevCmd's `WindowsSdkVerBinPath` /
`WindowsSDKVersion`, and **PowerShell 7+ (`pwsh`)**: every script here carries
`#requires -Version 7.0` and will not start under 5.1. The pack is ANTfrastructure's
`Invoke-MsixPackage`; this repo supplies only the staging directory (the release exe,
the DLLs beside it, `resources/`) and the token map.

**The route is `Build-Windows.ps1`'s MSIX Packaging step.** It takes every value from
the `Msix` block of `scripts/windows/Build-Windows.config.psd1`, and an environment
variable overrides each one (`MSIX_PACKAGE_NAME`, `MSIX_DISPLAY_NAME`, …); `-SkipMsix`
turns it off. The package lands in `dist\windows-<x64|arm64>\msix\`.

```pwsh
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\Build-Windows.ps1
```

**The same step signs**, with the first `*.pfx` at the repository root (`*.pfx` is
gitignored) and `MSIX_PFX_PASSWORD`, then verifies the signature. With no `.pfx` it warns
and the package stays unsigned, which is what CI builds. ANTfrastructure's
`GenerateCertificateMSIX.ps1` makes a test certificate; its `-Publisher` must be the
manifest's publisher, `CN=Kataglyphis` (the `Msix` block's `Publisher`):

```pwsh
pwsh -File .\third_party\ANTfrastructure\windows\scripts\certificates\GenerateCertificateMSIX.ps1 `
  -Password '<TEST_CERT_PASSWORD>' -Publisher 'CN=Kataglyphis' -PfxPath .\Kataglyphis.OxidANT.testcert.pfx
$env:MSIX_PFX_PASSWORD = '<TEST_CERT_PASSWORD>'
pwsh -ExecutionPolicy Bypass -File .\scripts\windows\Build-Windows.ps1
```

An existing PFX works the same way; its subject must match the publisher too. The
package is `dist\windows-<arch>\msix\Kataglyphis.OxidANT_<VERSION>_<arch>.msix`, with
`Cargo.toml`'s `[workspace.package]` version padded to four parts (`3.0.0` becomes
`3.0.0.0`). ANTfrastructure's standalone `New-MsixPackage.ps1` cannot package this repo:
it fills `__PACKAGE_NAME__`-style tokens, while `packaging/msix/AppxManifest.template.xml`
carries the `__MSIX_*__` tokens that `Build-Windows.ps1` fills.

Installing a test-signed package needs its certificate trusted machine-wide first, in
`LocalMachine\Root` **and** `LocalMachine\TrustedPeople`, from an elevated PowerShell.
The steps and the errors they prevent (`0x800B0109`, `Access denied`, `Get-AppxLog`):
[ANTfrastructure's certificates README](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/windows/scripts/certificates/README.md#trust-it-then-install-a-test-signed-package).
With this repo's names (repo-relative, so they hold in any checkout):

```pwsh
Add-AppxPackage -Path 'dist\windows-x64\msix\Kataglyphis.OxidANT_3.0.0.0_x64.msix'
```

Check, launch, update, remove:

```pwsh
Get-AppxPackage -Name Kataglyphis.OxidANT | Select-Object Name, PackageFullName, Status

$pkg = Get-AppxPackage -Name Kataglyphis.OxidANT
Start-Process "shell:AppsFolder\$($pkg.PackageFamilyName)!App"

# update: raise the version in Cargo.toml, rebuild and sign, then install it the same way
Add-AppxPackage -Path dist\windows-x64\msix\Kataglyphis.OxidANT_<NEW_VERSION>_x64.msix

Get-AppxPackage -Name Kataglyphis.OxidANT | Remove-AppxPackage
```

**The package identity changed on 2026-09-05** from `Kataglyphis.RustProjectTemplate`
to `Kataglyphis.OxidANT`. Windows treats the two as different applications, so it does
not upgrade an installation from before that date: uninstall it first.

## MSI

The **MSI Packaging** step of `Build-Windows.ps1` (off with `-SkipMsi`, or
`Msi.Enabled = $false` in `scripts/windows/Build-Windows.config.psd1`).

Output: `dist\windows-<x64|arm64>\msi\kataglyphis_cli-<VERSION>-<x64|arm64>.msi`

It installs the portable bundle's tree: the exe with every DLL of its closure beside it
(the chain ONNX Runtime and the VC++ runtime included), the GStreamer plugins in
`lib\gstreamer-1.0` and `resources\` with the model. `wix/main.wxs` holds the exe, the
licence and the shortcut; the rest is a component group the build generates, one
component per file.

The shared icon helper wraps `images/logo.png` as an ICO for the Start-menu shortcut
and Add/Remove Programs. The ICO is generated for each build from the project source.
The CLI's Windows executable also embeds `images/logo.ico` through the hub's Rust build
helper; the icon test checks that this checked-in ICO matches `images/logo.png`.

Built with the **WiX Toolset's `wix.exe build`**, not `cargo-wix`: cargo-wix drives WiX
v3's `candle.exe`/`light.exe` even in its newest release (0.3.9), while the image ships
WiX as a single `wix.exe` (`WIX_VERSION` in ANTfrastructure's `versions.env`, 7.0.0 on
2026-10-19). So `wix/main.wxs` is in the v4 schema, which WiX 7 still reads, and takes
version, binary path and licence path as preprocessor variables. The
`WixUI_FeatureTree` dialog set comes from `WixToolset.UI.wixext`, installed in the image.

## Linux: tar, deb, AppImage, flatpak

The lanes' *Create packages* step, inside the family Linux image (`--privileged`, which
flatpak-builder needs), after the release build:

```bash
bash scripts/linux/package-linux.sh --binary-path target/release/kataglyphis_cli \
  --version 3.0.0 --arch x64 --platform linux/amd64
```

It stages the bundle `oxidant/` (the CLI as `oxidant`, `lib/` with its closure and
`lib/gstreamer-1.0`, `resources/models`), proves it (`elf-closure.sh --check`, the G6
census, `gstreamer-bundle.sh check`, and the bundle's own `--help`, `onnx-runtime` and
`media-check` in an emptied environment), then packs `dist/oxidant-<version>-linux_<arch>-<x64|arm64>.tar.gz`,
the `.deb` (`/opt/oxidant`, `/usr/bin/oxidant`), the AppImage and the flatpak
(`org.kataglyphis.oxidant`, with `--device=all` for the camera). The rules behind each
part: [AGENTS.md § The bin was renamed](#the-bin-was-renamed-not-the-lib--do-not-undo-it).
The `clean-host` job then installs, unpacks and starts them on a bare Ubuntu 26.04 runner.

## The bin was renamed, not the lib — do not undo it

**The pdb collision is fixed; do not bring it back by renaming the bin.** Cargo warned that the root **lib** and the CLI **bin**, both then named `kataglyphis_rustprojecttemplate`, wrote the same `kataglyphis_rustprojecttemplate.pdb` (from the lib's `cdylib` crate type, not the rlib), and that this *"may become a hard error in the future"* ([rust-lang/cargo#6313](https://github.com/rust-lang/cargo/issues/6313)). A bin named `oxidant` today would collide on `oxidant.pdb`.

The **bin** was renamed, not the lib, on purpose: the consumers name the lib (then the C++ Vulkan engine through Corrosion, today OmniAccelerANT's Cargokit and frb loader), so `[lib] name` decides DLL/LIB filenames another repository depends on. What moved with the bin: `Msix.Binary` and `Msi.OutputName` in `scripts/windows/Build-Windows.config.psd1`, `File Name=` in `wix/main.wxs`, `BINARY_FILE` in the Linux lane (`reusable-linux.yml` today) and `BINARY` in the Windows one (then `rust_windows2025.yml`), the `-Binary` default in `Invoke-AppProfiles.ps1`, and `--bin` in `scripts/linux/run-person-detection.sh`. Since 2026-09-25 the Windows lanes set no `BINARY`; the exe name sits in their `artifact-name`, `host-command` and `run-command` (`windows-x64.yml`, `windows-arm64-cross.yml`).

**`[lib] name` did change later, on 2026-09-05**, when the repository became OxidANT: `[package] name` and `[lib] name` are both `oxidant`, so the artefacts are `oxidant.dll` / `liboxidant.so` / `liboxidant.a`. That is the outside-this-repo break the paragraph above warns about, and it was safe only because every consumer moved in the same commit: OmniAccelerANT's Cargokit wiring, podspecs, `Get-WindowsBuildConfig.ps1` and the committed `frb_generated.dart` loader stem, plus BeschleunigerBallett's Corrosion import. Another rename means finding them again: [Consumers](../AGENTS.md#consumers).

**`BINARY` means different things per platform.** On **Windows** it is an optional environment override of `Msix.Binary`, the executable `Build-Windows.ps1` builds and packages (`kataglyphis_cli`). In the **Linux** workflow (`reusable-linux.yml`, which both Linux lanes call) it is `oxidant` and names every Linux package and the command each installs: `scripts/linux/package-linux.sh` puts `target/release/$BINARY_FILE` into the bundle `oxidant/` as `oxidant`, packs that directory as the tar, `/opt/oxidant/` behind `/usr/bin/oxidant` in the deb, the AppImage's payload, and `org.kataglyphis.oxidant`'s command in the flatpak. So `BINARY_FILE` is the cargo artefact, `BINARY` what a user invokes.

**The Linux packages carry their runtime closure** (2026-10-20), by the hub's rules ([`linux-build-basics.md` § *Linux packages that run outside the image*](../third_party/ANTfrastructure/docs/linux-build-basics.md#linux-packages-that-run-outside-the-image)), since the CLI links GStreamer and loads ORT: the hub's `elf-closure.sh` puts the image's GStreamer, GLib, FFmpeg and GCC runtime into `lib/` with `$ORIGIN` RUNPATHs, the chain ORT goes in unpatched, `gstreamer-bundle.sh` copies the GUI pipeline's plugins into `lib/gstreamer-1.0`, `bundle-loader.sh --beside` leaves the image's glibc for an older host, and the model sits in `resources/models`. Before packing, `elf-closure.sh --check`, the G6 census, `gstreamer-bundle.sh check` and the bundle's own `--help`, `onnx-runtime` and `media-check` must pass; the `clean-host` job then installs the `.deb`, unpacks the tarball and starts the AppImage on a bare runner. `kataglyphis_media::ensure_gst_initialized` sets the four GStreamer variables that keep a package to its own plugins, each only when unset.

Every Linux and Windows package carries the project's `LICENSE`, staged with ANTfrastructure's shared helpers; a missing or empty source is fatal.
The MSI verifier resolves each license's file identifier from its source path in the generated WiX fragment before checking its bytes; do not assume payload order or a unique basename. The AppImage verifier runs on the matching native architecture and checks the extracted license.
The MSIX uses only `images/logo.png`; missing, malformed or transparent project logos fail the selected package step.
The MSI's installed RTF is generated from the repository `LICENSE` through ANTfrastructure's CMake encoder; retain the existing installer flow.
Linux packages verify every bundled ORT ELF against one wheel before shipping that wheel's original license and notices; retain the shared verifier and mutation workflow.
MSI packaging converts `images/logo.png` with the hub icon module and passes it to WiX for the shortcut and Add/Remove Programs.
The CLI's Windows PE resource uses the hub Rust build helper and `images/logo.ico`; the icon fixture rejects drift from `images/logo.png`.
Every package carries `licenses/cargo/` from the hub's `stage-cargo-licenses.py` for the CLI's target graph. A crate without a license file or with an `AND` expression needs an entry in `packaging/cargo-license-policy.json` naming an original upstream text under `packaging/licenses/cargo/`; never write or reconstruct a notice ([docs/packaging.md](packaging.md)).
