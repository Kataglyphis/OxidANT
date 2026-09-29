#requires -Version 7.0
<#
.SYNOPSIS
  One Windows CI lane of this repo, inside the family image: what windows-x64.yml and
  windows-arm64-cross.yml run through ANTfrastructure's reusable container-ci-windows.yml.
.DESCRIPTION
  amd64, the host arch:
    1. Invoke-DebugTests.ps1: the debug unit, integration and fuzz tests;
    2. Invoke-WindowsConfigMatrix.ps1: every app configuration built, the plain one run;
    3. Build-Windows.ps1 -SkipTests: audit/deny, fmt, clippy, the release build and the
       packages in dist\windows-x64.
  arm64, the cross build: Build-Windows.ps1 alone. Nothing arm64 runs on this host, and the
  source gates are the x64 lane's.
  Each step runs in its own pwsh, as it did as its own workflow step, and the first failure
  stops the lane. A local container run executes exactly this.
#>
[CmdletBinding()]
param(
  # amd64 (alias x64) or arm64; empty takes the image's WINDOWS_TARGET_ARCH.
  [string]$TargetArch = ''
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

# The CI compiler cache (container-ci-windows.yml's compiler-cache-key) arrives as
# CI_COMPILER_CACHE in the workspace. sccache cannot write to that mount, so the cache moves to
# the container-local dirs Initialize-BuildCacheEnvironment uses, and every step compiles
# through it, not only Build-Windows.ps1. It moves back even after a failed step.
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
} finally {
  if ($ciCache) {
    & sccache --show-stats | Out-Host
    & sccache --stop-server 2>&1 | Out-Null
    $cargoHome = Join-Path $cacheRoot 'cargo'
    Move-CacheTree -From (Join-Path $cacheRoot 'sccache') -To (Join-Path $ciCache 'sccache')
    Move-CacheTree -From $cargoHome -To (Join-Path $ciCache 'cargo') -ExcludeDirs @((Join-Path $cargoHome 'registry\src'), (Join-Path $cargoHome 'git\checkouts'))
  }
}
