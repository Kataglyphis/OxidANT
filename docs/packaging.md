# Windows packages: MSIX and MSI

Both are steps of `Build-Windows.ps1`, beside the portable bundle in
`dist\windows-<x64|arm64>\`. Each carries the exe's whole DLL closure, the GUI's
GStreamer plugins and the model: [AGENTS.md § Continuous integration](../AGENTS.md#continuous-integration).

## MSIX

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
with `Cargo.toml`'s `[workspace.package]` version padded to four parts (`2.3.4` becomes `2.3.4.0`).
ANTfrastructure's standalone `New-MsixPackage.ps1` cannot package this repo: it fills
`__PACKAGE_NAME__`-style tokens, while `packaging/msix/AppxManifest.template.xml`
carries the `__MSIX_*__` tokens that `Build-Windows.ps1` fills.

Installing a test-signed package needs its certificate trusted machine-wide first,
in `LocalMachine\Root` **and** `LocalMachine\TrustedPeople`, from an elevated
PowerShell. The steps and the errors they prevent (`0x800B0109`, `Access denied`,
`Get-AppxLog`) are in
[ANTfrastructure's certificates README](https://github.com/Kataglyphis/ANTfrastructure/blob/develop/windows/scripts/certificates/README.md#trust-it-then-install-a-test-signed-package).
With this repo's names (repo-relative, so they hold in any checkout):

```pwsh
Add-AppxPackage -Path 'dist\windows-x64\msix\Kataglyphis.OxidANT_2.3.4.0_x64.msix'
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

**The package identity changed on 2026-09-05** from
`Kataglyphis.RustProjectTemplate` to `Kataglyphis.OxidANT`. Windows treats the two
as different applications, so an installation predating that date is not upgraded —
uninstall it first.

## MSI

Runs as the **MSI Packaging** step of `Build-Windows.ps1` (disable with `-SkipMsi`,
or `Msi.Enabled = $false` in `scripts/windows/Build-Windows.config.psd1`).

Output: `dist\windows-<x64|arm64>\msi\kataglyphis_cli-<VERSION>-<x64|arm64>.msi`

It installs the portable bundle's tree: the exe with every DLL of its closure
beside it (the chain ONNX Runtime and the VC++ runtime included), the GStreamer
plugins in `lib\gstreamer-1.0` and `resources\` with the model. `wix/main.wxs`
holds the exe, the licence and the shortcut; the rest is a component group the
build generates, one component per file.

Built with the **WiX Toolset's `wix.exe build`**, not `cargo-wix`: cargo-wix drives
WiX v3's `candle.exe`/`light.exe` even in its newest release (0.3.9), while the
container image ships WiX as a single `wix.exe` (`WIX_VERSION` in ANTfrastructure's
`versions.env`, 7.0.0 on 2026-10-19). `wix/main.wxs` is therefore
in the v4 schema, which WiX 7 still reads, and gets version, binary path and licence
path as preprocessor variables. The `WixUI_FeatureTree` dialog set comes from
`WixToolset.UI.wixext`, already installed in the image.
