#requires -Version 7.0
<#
.SYNOPSIS
  Builds a separate unverified Windows x64 GPU portable candidate.
#>
[CmdletBinding()]
param([Parameter(Mandatory)][ValidateSet('nvidia', 'amd')][string]$Vendor)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repo = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$provider = if ($Vendor -eq 'nvidia') { 'cuda' } else { 'directml' }
& (Join-Path $PSScriptRoot 'Test-GpuCandidateInputs.ps1') -Image -Provider $provider
& pwsh -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'Build-Windows.ps1') -TargetArch amd64 -SkipTests -SkipMsix -SkipMsi
if ($LASTEXITCODE -ne 0) { throw "Windows build failed: $LASTEXITCODE" }

$source = Join-Path $repo 'dist/windows-x64/bundle'
$output = Join-Path $repo "dist/gpu-$Vendor"
if (Test-Path -LiteralPath $output) { throw "Candidate output already exists: $output" }
$null = New-Item -ItemType Directory -Path $output
$bundle = Join-Path $output 'bundle'
& (Join-Path $PSScriptRoot 'New-GpuCandidate.ps1') -SourceBundle $source -Destination $bundle -Provider $provider

$zip = Join-Path $output "oxidant-$provider-$Vendor-windows-x64-unverified.zip"
Compress-Archive -Path (Join-Path $bundle '*') -DestinationPath $zip -CompressionLevel Optimal
& (Join-Path $PSScriptRoot 'Test-GpuCandidateArchive.ps1') -Bundle $bundle -Archive $zip -Provider $provider
Write-Host "Unverified $provider candidate: $zip"
