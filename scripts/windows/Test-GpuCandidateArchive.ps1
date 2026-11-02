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
Import-BuildModule @('WindowsOrtPayload.Common', 'WindowsOrtProvenance.Common', 'WindowsOrtLicense.Common', 'WindowsPackageBinary.Common')
if (-not (Get-Command Assert-PackageOrtFamily -ErrorAction SilentlyContinue)) {
  throw 'The pinned ANTfrastructure lacks complete ORT-family package verification.'
}
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$hub = Join-Path $repoRoot 'third_party\ANTfrastructure'
$bundleRoot = (Resolve-Path -LiteralPath $Bundle).Path
$modelFiles = @(& (Join-Path $PSScriptRoot 'Get-ModelLockFile.ps1'))
& python -I (Join-Path $hub 'shared\python\verify_locked_files.py') --root $bundleRoot `
  --directory (Join-Path $bundleRoot 'resources\models') --lock (Join-Path $repoRoot 'resources\models\models.lock') `
  --fetcher (Join-Path $hub 'linux\scripts\lib\fetch-data-lock.py') `
  @($modelFiles | ForEach-Object { '--file', $_ })
if ($LASTEXITCODE -ne 0) { throw 'The finished candidate model files do not match models.lock.' }
$expected = @('onnxruntime_providers_shared.dll')
if ($Provider -eq 'cuda') { $expected += 'onnxruntime_providers_cuda.dll' }
$actual = @(Get-PackageOrtProviderNames -Path $Archive)
if (@(Compare-Object -ReferenceObject $expected -DifferenceObject $actual -CaseSensitive).Count -ne 0) {
  throw "GPU archive has an unexpected ONNX Runtime provider set: $($actual -join ', ')"
}
$required = @('onnxruntime.dll') + $expected
if ($Provider -eq 'directml') { $required += 'DirectML.dll' }
$extra = @()
if ($Provider -eq 'cuda' -and (Test-Path -LiteralPath (Join-Path $bundleRoot 'DirectML.dll') -PathType Leaf)) {
  $extra += 'DirectML.dll'
}
$allowed = @($required + $extra)
Assert-PackageOrtFamily -Path $bundleRoot -Name $allowed
Assert-PackageOrtFamily -Path $Archive -Name $allowed
$members = @('kataglyphis_cli.exe', 'LICENSE', "oxidant-$Provider.cmd", "$($Provider.ToUpperInvariant())-README.txt") +
  @($modelFiles | ForEach-Object { "resources/models/$_" }) + $required + $extra
$paths = @($members | ForEach-Object { Join-Path $Bundle $_ })
Assert-PackageBinary -Path $Archive -Member $members -SourcePath $paths
$proof = Join-Path ([IO.Path]::GetTempPath()) "oxidant-gpu-wheel-proof-$([guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $proof
try {
  $providers = @($expected | ForEach-Object { Join-Path $bundleRoot $_ })
  $directMlPath = Join-Path $bundleRoot 'DirectML.dll'
  $directMl = if (Test-Path -LiteralPath $directMlPath -PathType Leaf) { $directMlPath } else { '' }
  $wheelLicenses = Copy-OrtWheelLicenses -OrtDll (Join-Path $bundleRoot 'onnxruntime.dll') `
    -ProviderDll $providers -DirectMlDll $directMl -WheelDir (Get-OrtChainWheelStore) -Destination $proof
  foreach ($name in 'LICENSE', 'ThirdPartyNotices.txt') {
    $sourceHash = (Get-FileHash -LiteralPath (Join-Path $bundleRoot "licenses/onnxruntime/$name") -Algorithm SHA256).Hash
    $wheelHash = (Get-FileHash -LiteralPath (Join-Path $wheelLicenses $name) -Algorithm SHA256).Hash
    if ($sourceHash -cne $wheelHash) { throw "GPU bundle ORT notice differs from the matching wheel: $name" }
  }
} finally {
  foreach ($path in @('licenses/onnxruntime/LICENSE', 'licenses/onnxruntime/ThirdPartyNotices.txt', 'licenses/onnxruntime', 'licenses', '')) {
    $target = if ($path) { Join-Path $proof $path } else { $proof }
    Remove-Item -LiteralPath $target -Force -ErrorAction SilentlyContinue
  }
}
Assert-OrtWheelLicensesInPackage -Path $Archive -LicenseDirectory (Join-Path $Bundle 'licenses/onnxruntime') -Format archive
Write-Host "GPU archive matches its proven $Provider bundle and provider set."
