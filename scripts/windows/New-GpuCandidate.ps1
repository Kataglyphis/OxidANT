#requires -Version 7.0
<#
.SYNOPSIS
  Creates a separate GPU portable candidate from an already built Windows bundle.
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$SourceBundle,
  [Parameter(Mandatory)][string]$Destination,
  [ValidateSet('cuda', 'directml')][string]$Provider = 'directml'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsTargetArch.Common', 'WindowsOrtPayload.Common', 'WindowsOrtProvenance.Common', 'WindowsOrtLicense.Common')

$source = (Resolve-Path -LiteralPath $SourceBundle).Path.TrimEnd('\', '/')
$destinationInput = if ([IO.Path]::IsPathRooted($Destination)) { $Destination } else { Join-Path (Get-Location).Path $Destination }
$destinationPath = [IO.Path]::GetFullPath($destinationInput).TrimEnd('\', '/')
if (-not (Test-Path -LiteralPath $source -PathType Container)) { throw "Not a bundle directory: $source" }
if (Test-Path -LiteralPath $destinationPath) { throw "Destination already exists: $destinationPath" }
$comparison = [StringComparison]::OrdinalIgnoreCase
if ($destinationPath.StartsWith("$source\", $comparison) -or $source.StartsWith("$destinationPath\", $comparison)) {
  throw 'Source and destination must not contain one another.'
}
$required = @('kataglyphis_cli.exe', 'onnxruntime.dll', 'onnxruntime_providers_shared.dll',
    'LICENSE', 'resources\models\deim_dfine_n_coco.onnx',
    'licenses\onnxruntime\LICENSE', 'licenses\onnxruntime\ThirdPartyNotices.txt')
if ($Provider -eq 'directml') { $required += 'DirectML.dll' }
foreach ($relative in $required) {
  $file = Get-Item -LiteralPath (Join-Path $source $relative) -ErrorAction Stop
  if ($file.PSIsContainer -or $file.Length -eq 0) { throw "Missing or empty candidate file: $relative" }
}
$peNames = @('kataglyphis_cli.exe', 'onnxruntime.dll', 'onnxruntime_providers_shared.dll')
if ($Provider -eq 'directml') { $peNames += 'DirectML.dll' }
$peFiles = $peNames |
  ForEach-Object { Join-Path $source $_ }
$null = Assert-PeTargetMachine -Path $peFiles -Arch 'amd64' -Context 'GPU candidates require Windows x64'
if (-not (Test-PayloadLoadsOrt -ExePath (Join-Path $source 'kataglyphis_cli.exe') -IncludeDirectory 'lib')) {
  throw 'The source executable and its payload do not load ONNX Runtime.'
}
$null = Assert-ChainOrtTree -Root $source
& (Join-Path $PSScriptRoot 'Test-GpuCandidateInputs.ps1') -Image -Provider $Provider
$parent = Split-Path -Parent $destinationPath
$null = New-Item -ItemType Directory -Path $parent -Force
Copy-Item -LiteralPath $source -Destination $destinationPath -Recurse
if ($Provider -eq 'cuda') {
  $null = Copy-ChainOrtBeside -OnnxRoot $env:ONNX_ROOT -Destination $destinationPath -RequiredProvider 'onnxruntime_providers_cuda.dll'
}
$sourceOrtLicense = Join-Path $source 'licenses\onnxruntime'
$originalHashes = @{}
foreach ($name in 'LICENSE', 'ThirdPartyNotices.txt') {
  $originalHashes[$name] = (Get-FileHash -LiteralPath (Join-Path $sourceOrtLicense $name) -Algorithm SHA256).Hash
}
$providerDlls = @((Join-Path $destinationPath 'onnxruntime_providers_shared.dll'))
if ($Provider -eq 'cuda') { $providerDlls += Join-Path $destinationPath 'onnxruntime_providers_cuda.dll' }
$directMl = if ($Provider -eq 'directml') { Join-Path $destinationPath 'DirectML.dll' } else { '' }
$licenseDir = Copy-OrtWheelLicenses -OrtDll (Join-Path $destinationPath 'onnxruntime.dll') `
  -ProviderDll $providerDlls -DirectMlDll $directMl `
  -WheelDir (Get-OrtChainWheelStore) -Destination $destinationPath
foreach ($name in 'LICENSE', 'ThirdPartyNotices.txt') {
  if ((Get-FileHash -LiteralPath (Join-Path $licenseDir $name) -Algorithm SHA256).Hash -ne $originalHashes[$name]) {
    throw "Source ORT notice does not match its chain wheel: $name"
  }
}
Assert-OrtWheelLicensesInPackage -Path $destinationPath -LicenseDirectory $licenseDir -Format archive
$null = Assert-ChainOrtTree -Root $destinationPath
Copy-Item -LiteralPath (Join-Path $repoRoot "packaging\windows\oxidant-$Provider.cmd") -Destination $destinationPath
Copy-Item -LiteralPath (Join-Path $repoRoot "packaging\windows\$($Provider.ToUpperInvariant())-README.txt") -Destination $destinationPath
Write-Host "$Provider portable candidate: $destinationPath"
Write-Host "Run oxidant-$Provider.cmd; GPU execution requires compatible host drivers and has not been verified by packaging."
