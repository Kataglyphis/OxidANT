#requires -Version 7.0
<#
.SYNOPSIS
  One Windows CI lane inside the family image, as windows-x64.yml and windows-arm64-cross.yml run it.
.DESCRIPTION
  amd64: debug tests, the CLI built and run in debug and release, then Build-Windows.ps1 -SkipTests; arm64: Build-Windows.ps1 alone.
  amd64 compiles every crate's C and C++ with the image's clang-cl (CC, CXX), never MSVC's cl.
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
Import-BuildModule @('WindowsTargetArch.Common', 'WindowsBuild.Common')
$arch = Get-WindowsTargetArch -Arch $TargetArch

# Unset, cc-rs takes MSVC's cl, under which tract-linalg's clang-cl break stayed hidden (third_party/tract-linalg-0.23.8).
if (-not (Test-WindowsCrossTarget -Arch $arch)) {
  $clangCl = (Get-Command clang-cl -ErrorAction Stop).Source
  $env:CC = 'clang-cl'
  $env:CXX = 'clang-cl'
  Write-Host "C and C++ through $clangCl (CC, CXX)"
}

function Invoke-LaneStep {
  param([Parameter(Mandatory)][string]$Script, [string[]]$Arguments = @())
  Write-Host "==> $Script $($Arguments -join ' ')"
  & pwsh -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot $Script) @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$Script failed with exit code $LASTEXITCODE" }
}

# sccache cannot write to the CI_COMPILER_CACHE mount, so both caches move in for all steps and back after.
$restored = Restore-CiCompilerCache -Cache sccache, cargo
if ($restored) {
  $env:RUSTC_WRAPPER = (Get-Command sccache -ErrorAction Stop).Source
  if ($env:SCCACHE_ERROR_LOG) { New-Item -ItemType Directory -Force -Path (Split-Path $env:SCCACHE_ERROR_LOG) | Out-Null }
}

try {
  if (-not (Test-WindowsCrossTarget -Arch $arch)) {
    Invoke-LaneStep -Script 'Invoke-DebugTests.ps1'
    # No profile build: fat LTO plus debuginfo of the whole CLI runs a 16 GB runner out of LLVM memory (run 39471292642).
    Invoke-LaneStep -Script 'Invoke-AppProfiles.ps1' -Arguments @('-Profiles', 'debug,release')
  }
  Invoke-LaneStep -Script 'Build-Windows.ps1' -Arguments @('-SkipTests', '-TargetArch', $arch)
  if ($StageTests) { Invoke-LaneStep -Script 'Stage-CrossTests.ps1' -Arguments @('-TargetArch', $arch) }
} finally {
  $null = Save-CiCompilerCache -Restored $restored
}
