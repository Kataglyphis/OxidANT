#requires -Version 7.0
<#
.SYNOPSIS
  Checks pinned packaging APIs and, in an image, the x64 GPU wheel inputs.
#>
[CmdletBinding()]
param([switch]$Image, [ValidateSet('cuda', 'directml')][string]$Provider = 'directml')

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$hub = Join-Path $repo 'third_party/ANTfrastructure'
$binaryVerifier = Join-Path $hub 'linux/scripts/verify_package_binary.py'
if (-not (Test-Path -LiteralPath $binaryVerifier -PathType Leaf)) {
  throw 'The pinned ANTfrastructure lacks the finished-package binary verifier.'
}
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsTargetArch.Common', 'WindowsOrtProvenance.Common', 'WindowsOrtPayload.Common', 'WindowsOrtLicense.Common', 'WindowsPackageBinary.Common')
if (-not (Get-Command Copy-OrtWheelLicenses).Parameters.ContainsKey('ProviderDll')) {
  throw 'The pinned ANTfrastructure lacks same-wheel provider verification.'
}
if (-not (Get-Command Copy-OrtWheelLicenses).Parameters.ContainsKey('DirectMlDll')) {
  throw 'The pinned ANTfrastructure lacks same-wheel DirectML verification.'
}
if (-not (Get-Command Copy-ChainOrtBeside).Parameters.ContainsKey('RequiredProvider')) {
  throw 'The pinned ANTfrastructure lacks selected-provider staging.'
}
if (-not (Get-Command Get-ChainOrtRuntimeFile -ErrorAction SilentlyContinue)) {
  throw 'The pinned ANTfrastructure lacks the chain runtime path resolver.'
}
if (-not (Get-Command Get-PackageOrtProviderNames -ErrorAction SilentlyContinue)) {
  throw 'The pinned ANTfrastructure lacks finished-package provider-set verification.'
}
if (-not $Image) { return }
if ([string]::IsNullOrWhiteSpace($env:ONNX_ROOT)) { throw 'ONNX_ROOT is not set in the GPU image.' }
$names = @('onnxruntime.dll', 'onnxruntime_providers_shared.dll')
if ($Provider -eq 'cuda') { $names += 'onnxruntime_providers_cuda.dll' } else { $names += 'DirectML.dll' }
$files = @($names | ForEach-Object {
    $path = Get-ChainOrtRuntimeFile -OnnxRoot $env:ONNX_ROOT -Name $_
    $file = Get-Item -LiteralPath $path -ErrorAction Stop
    if ($file.PSIsContainer -or $file.Length -eq 0) { throw "Missing or empty image runtime file: $_" }
    $file.FullName
  })
$null = Assert-PeTargetMachine -Path $files -Arch amd64 -Context 'GPU candidates require Windows x64'
$tempRoot = (Resolve-Path -LiteralPath ([IO.Path]::GetTempPath())).ProviderPath.TrimEnd('\', '/')
$scratch = Join-Path $tempRoot "oxidant-gpu-input-$([guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $scratch -ErrorAction Stop
try {
  $providerDlls = @($files[1]) + @(if ($Provider -eq 'cuda') { $files[2] })
  $directMl = if ($Provider -eq 'directml') { $files[2] } else { '' }
  $null = Copy-OrtWheelLicenses -OrtDll $files[0] -ProviderDll $providerDlls `
    -DirectMlDll $directMl -WheelDir (Get-OrtChainWheelStore) -Destination $scratch
} finally {
  $resolved = (Resolve-Path -LiteralPath $scratch).ProviderPath
  if (-not $resolved.StartsWith("$tempRoot\", [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove scratch outside $tempRoot`: $resolved"
  }
  Remove-Item -LiteralPath $resolved -Recurse -Force
}
