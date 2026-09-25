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

if (-not (Test-WindowsCrossTarget -Arch $arch)) {
  Invoke-LaneStep -Script 'Invoke-DebugTests.ps1'
  Invoke-LaneStep -Script 'Invoke-WindowsConfigMatrix.ps1'
}
Invoke-LaneStep -Script 'Build-Windows.ps1' -Arguments @('-SkipTests', '-TargetArch', $arch)
