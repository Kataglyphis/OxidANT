#requires -Version 7.0
# Verify the finished portable GPU archive against its wheel-proven source bundle.
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$Bundle,
  [Parameter(Mandatory)][string]$Archive,
  [Parameter(Mandatory)][ValidateSet('cuda', 'directml')][string]$Provider
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsOrtPayload.Common', 'WindowsOrtLicense.Common', 'WindowsPackageBinary.Common')
$expected = @('onnxruntime_providers_shared.dll')
if ($Provider -eq 'cuda') { $expected += 'onnxruntime_providers_cuda.dll' }
$actual = @(Get-PackageOrtProviderNames -Path $Archive)
if (@(Compare-Object -ReferenceObject $expected -DifferenceObject $actual -CaseSensitive).Count -ne 0) {
  throw "GPU archive has an unexpected ONNX Runtime provider set: $($actual -join ', ')"
}
$required = @('onnxruntime.dll') + $expected
if ($Provider -eq 'directml') { $required += 'DirectML.dll' }
$extra = @(Get-OrtFamilyFile -Directory $Bundle | Where-Object { $_.Name -notin $required } | ForEach-Object Name)
$members = @('kataglyphis_cli.exe', 'LICENSE', 'resources/models/deim_dfine_n_coco.onnx',
    "oxidant-$Provider.cmd", "$($Provider.ToUpperInvariant())-README.txt") + $required + $extra
$paths = @($members | ForEach-Object { Join-Path $Bundle $_ })
Assert-PackageBinary -Path $Archive -Member $members -SourcePath $paths
Assert-OrtWheelLicensesInPackage -Path $Archive -LicenseDirectory (Join-Path $Bundle 'licenses/onnxruntime') -Format archive
Write-Host "GPU archive matches its proven $Provider bundle and provider set."
