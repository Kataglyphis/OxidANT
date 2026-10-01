#requires -Version 7.0
<#
.SYNOPSIS
  One Windows CI lane inside the family image, as windows-x64.yml and windows-arm64-cross.yml run it.
.DESCRIPTION
  amd64: debug tests, the config matrix, then Build-Windows.ps1 -SkipTests; arm64: Build-Windows.ps1 alone.
  Each step runs in its own pwsh and the first failure stops the lane.
#>
[CmdletBinding()]
param(
  # amd64 (alias x64) or arm64; empty takes the image's WINDOWS_TARGET_ARCH.
  [string]$TargetArch = '',
  # Cross only: after the build, stage the x64 lane's tests for the target (Stage-CrossTests.ps1, hub CON43).
  [switch]$StageTests
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsTargetArch.Common')
$arch = Get-WindowsTargetArch -Arch $TargetArch

function Invoke-LaneStep {
  param([Parameter(Mandatory)][string]$Script, [string[]]$Arguments = @())
  Write-Host "==> $Script $($Arguments -join ' ')"
  & pwsh -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot $Script) @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Script failed with exit code $LASTEXITCODE" }
}

# robocopy /MOVE; exit codes below 8 are success.
function Move-CacheTree {
  param([Parameter(Mandatory)][string]$From, [Parameter(Mandatory)][string]$To, [string[]]$ExcludeDirs = @())
  if (-not (Test-Path -LiteralPath $From)) { return }
  $copyArgs = @($From, $To, '/E', '/MOVE', '/MT:16', '/R:1', '/W:1', '/NFL', '/NDL', '/NJH', '/NJS', '/NP')
  if ($ExcludeDirs) { $copyArgs += @('/XD') + $ExcludeDirs }
  & robocopy @copyArgs | Out-Host
  if ($LASTEXITCODE -ge 8) { throw "robocopy $From -> $To failed with exit code $LASTEXITCODE" }
  $global:LASTEXITCODE = 0
}

# sccache cannot write to the CI_COMPILER_CACHE mount, so the cache moves in for all steps and back after.
$ciCache = $env:CI_COMPILER_CACHE
if ($ciCache) {
  $fastDir = if ($env:KATAGLYPHIS_FAST_BUILD_DIR) { $env:KATAGLYPHIS_FAST_BUILD_DIR } else { 'C:\kataglyphis_fast_build' }
  $cacheRoot = Join-Path $fastDir '.cache'
  foreach ($sub in 'sccache', 'cargo') { Move-CacheTree -From (Join-Path $ciCache $sub) -To (Join-Path $cacheRoot $sub) }
  $env:SCCACHE_DIR = Join-Path $cacheRoot 'sccache'
  $env:CARGO_HOME = Join-Path $cacheRoot 'cargo'
  $env:RUSTC_WRAPPER = (Get-Command sccache -ErrorAction Stop).Source
  if ($env:SCCACHE_ERROR_LOG) { New-Item -ItemType Directory -Force -Path (Split-Path $env:SCCACHE_ERROR_LOG) | Out-Null }
}

try {
  if (-not (Test-WindowsCrossTarget -Arch $arch)) {
    Invoke-LaneStep -Script 'Invoke-DebugTests.ps1'
    Invoke-LaneStep -Script 'Invoke-WindowsConfigMatrix.ps1'
  }
  Invoke-LaneStep -Script 'Build-Windows.ps1' -Arguments @('-SkipTests', '-TargetArch', $arch)
  if ($StageTests) { Invoke-LaneStep -Script 'Stage-CrossTests.ps1' -Arguments @('-TargetArch', $arch) }
} finally {
  if ($ciCache) {
    & sccache --show-stats | Out-Host
    & sccache --stop-server 2>&1 | Out-Null
    $cargoHome = Join-Path $cacheRoot 'cargo'
    Move-CacheTree -From (Join-Path $cacheRoot 'sccache') -To (Join-Path $ciCache 'sccache')
    Move-CacheTree -From $cargoHome -To (Join-Path $ciCache 'cargo') -ExcludeDirs @((Join-Path $cargoHome 'registry\src'), (Join-Path $cargoHome 'git\checkouts'))
  }
}
