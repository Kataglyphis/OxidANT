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
on a self-hosted `gpu-amd-rocm` runner and requires profiled MIGraphX operators.
The dispatch-only `Linux NVIDIA GPU package candidates` workflow targets x64 and
arm64 with the registry NVIDIA image. Its `verify-gpu` option requires both x64 and
arm64 self-hosted runners labelled `gpu-nvidia-cuda`; each must profile a CUDA model
operator. The hardware check profiles the finished tarball, extracted DEB payload, and
AppImage separately; it invokes the AppImage with extract-and-run, without FUSE. A GPU
runner needs `dpkg-deb` and temporary space for those payloads as well as the host GPU
runtime. The clean-host job also exercises the installed DEB command. Uploaded artifacts
remain labelled candidates even when profiling is skipped.
`package-linux.sh --ort-variant nvidia` or `--ort-variant amd` selects the chain GPU
ORT and its matching provider, and appends `+gpu-nvidia` or `+gpu-amd` to the version.
The DEBs all use the `oxidant` package identity and install the `oxidant` command;
installing a different variant replaces the existing one rather than installing both.
Linux GPU builds target NVIDIA on x64 and arm64, and AMD on x64; arm64 ROCm is skipped.
Pass `--package-types tar,deb,appimage`: Flatpak is excluded because these builds
require vendor libraries from the host. The launcher defaults to `cuda` or `migraphx`
and an explicit `KATAGLYPHIS_ORT_DEVICE` overrides it. The host must install a
compatible GPU driver and CUDA/cuDNN or ROCm/MIGraphX runtime, including its complete
transitive libraries and dynamic-loader configuration. These packages do not bundle
those vendor runtimes. Missing GPU support is an inference error, not a silent switch
to tract; ORT may execute individual unsupported operators on CPU.

A local AMD x64 build passed clean Ubuntu installation, CPU inference using the
bundled ORT for the DEB, tarball and AppImage, and a missing-ROCm failure check
for the installed DEB.
Both GPU workflows now require each finished DEB, tarball and AppImage to select its
vendor GPU by default and report a missing host runtime on the clean-host runner;
that expanded check has not yet run in the hosted workflow.
Actual GPU inference and the hosted workflow remain unverified. The published ROCm
image used locally lacked GTK development packages, so the workflow installs them
in the build container. Its 74.75 GB local image size makes hosted runner disk space
a concern, even after the shared full cleanup.

To verify operator placement on a GPU host, run the packaged command with
`onnx-check --profile /tmp/oxidant-profile` and the desired `KATAGLYPHIS_ORT_DEVICE`.
Then run `python3 scripts/verify-ort-gpu-profile.py <generated-json> --device migraphx`
from this checkout (use `cuda` for NVIDIA). A successful check proves at least one
model operator used that provider, not that every operator did.

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

The dispatch-only `windows-gpu-candidates.yml` uses the registry's NVIDIA CUDA and AMD
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

To profile a candidate on hardware, dispatch with `verify-gpu` and provide Windows x64
self-hosted runners labelled `gpu-nvidia-cuda` and `gpu-amd-directml`. Set
`amd-device-index` to the AMD adapter's DXGI index (default `0`); the job checks that
the same index is a physical AMD GPU before requesting it from DirectML. Each job
requires model operators in its matching ONNX Runtime provider.

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
part: [AGENTS.md § The bin was renamed](../AGENTS.md#the-bin-was-renamed-not-the-lib--do-not-undo-it).
The `clean-host` job then installs, unpacks and starts them on a bare Ubuntu 26.04 runner.
