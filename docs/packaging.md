# Packages

Every package carries what the CLI loads: the GStreamer libraries and the GUI's plugins,
the chain-built ONNX Runtime, the model, and on Linux the image's C and C++ runtime. On
Windows the MSIX and the MSI are steps of `Build-Windows.ps1`, beside the portable bundle in
`dist\windows-<x64|arm64>\` ([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration));
on Linux `scripts/linux/package-linux.sh` packs one bundle four ways
([§ Linux](#linux-tar-deb-appimage-flatpak)).

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
Linux tar, DEB and Flatpak read back every staged crate file and the model's `LICENSE` and
`NOTICE`; the AppImage, the Windows bundle and MSIX check the crate manifest, the CLI's own
entry and the model license; the MSI resolves each through the generated WiX fragment,
because the crate trees repeat the `LICENSE` basename. A Windows payload that ships the
chain `onnxruntime.dll` also carries the matching wheel's `LICENSE` and
`ThirdPartyNotices.txt` (`Copy-OrtWheelLicenses`), checked in every format. The bundled
native libraries (GStreamer, GLib, GTK, FFmpeg, the C and C++ runtimes) are not inventoried
yet.

The MSIX uses only `images/logo.png`. The shared MSIX stager rejects a missing,
malformed or fully transparent image instead of replacing it with another logo.
The MSI's installed `License.rtf` is generated from the current project `LICENSE`
by ANTfrastructure's CMake encoder; its existing installer flow is unchanged.

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
