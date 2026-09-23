#requires -Version 7.0

# The release zip: the exe plus, when it loads ONNX Runtime, the chain-built ORT
# Build-Windows.ps1 staged beside it, proved by the hub's G6 census. The hub's
# New-Archive.ps1 zips the exe alone.
param(
  [string]$Binary = $env:BINARY,
  [string]$Version = $env:VERSION,
  [string]$Platform = $env:PLATFORM,
  [string]$Arch = $env:ARCH,
  [string]$Workspace = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsOrtPayload.Common')
# G6, the hub's ORT census; a hub pin older than its ORT single-source commit lacks it.
try { Import-BuildModule @('WindowsOrtProvenance.Common') } catch { throw (Get-OrtCensusRequirement -Cause $_.Exception.Message) }

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if ([string]::IsNullOrWhiteSpace($Workspace)) { $Workspace = $repoRoot }
foreach ($required in @('Binary', 'Version', 'Platform', 'Arch')) {
  if ([string]::IsNullOrWhiteSpace((Get-Variable -Name $required -ValueOnly))) {
    throw "New-ReleaseArchive.ps1: -$required (or its environment variable) is required."
  }
}

$releaseDir = Join-Path $Workspace 'target\release'
$exePath = Join-Path $releaseDir "$Binary.exe"
if (-not (Test-Path -LiteralPath $exePath -PathType Leaf)) {
  throw "Release binary not found: $exePath"
}

# The zip ships the payload G6 just proved: the exe, plus the chain ORT when it loads one.
$payload = New-OrtProvenPayload -ExePath $exePath -Destination (Join-Path $Workspace 'target\ort-payload\zip')
$files = @($payload.Exe) + @($payload.OrtDlls)

$versionSafe = $Version -replace '^v', '' -replace '/', '-'
$distDir = Join-Path $Workspace 'dist'
New-Item -ItemType Directory -Force -Path $distDir | Out-Null
$archivePath = Join-Path $distDir "$Binary-$versionSafe-$Platform-$Arch.zip"
Compress-Archive -LiteralPath $files -DestinationPath $archivePath -Force
Write-Host "Archive created: $archivePath ($(($files | ForEach-Object { Split-Path $_ -Leaf }) -join ', '))"
