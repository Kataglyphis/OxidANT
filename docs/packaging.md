# Packages

Every package carries what the CLI loads: the GStreamer libraries and the GUI's plugins,
the chain-built ONNX Runtime, the model, and on Linux the image's C and C++ runtime. On
Windows the MSIX and the MSI are steps of `Build-Windows.ps1`, beside the portable bundle in
`dist\windows-<x64|arm64>\` ([AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration));
on Linux `scripts/linux/package-linux.sh` packs one bundle four ways
([§ Linux](#linux-tar-deb-appimage-flatpak)).

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
