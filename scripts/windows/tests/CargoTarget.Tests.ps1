#requires -Version 7.0

# WindowsCargoTarget.Common: every arch-dependent path and name Build-Windows.ps1 reads. The host (amd64)
# must keep today's layout byte for byte, since the x64 lane uploads from it; arm64 is the cross build.
# NOTE: written for Pester 3.4.0 - no BeforeAll outside Describe, dash-less Should.

Describe 'WindowsCargoTarget.Common' {

    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsTargetArch.Common'
    Import-BuildModule 'WindowsCargoTarget.Common'

    $target = 'C:\ct'
    $ws = 'C:\ws'

    It 'keeps the host layout: no --target, target\release, x64 packages under dist' {
        $l = Get-CargoTargetLayout -Arch 'amd64' -TargetRoot $target -WorkspacePath $ws
        $l.IsCross | Should Be $false
        @($l.CargoArgs).Count | Should Be 0
        $l.ArchTargetDir | Should Be 'C:\ct'
        $l.ReleaseDir | Should Be 'C:\ct\release'
        $l.PackageArch | Should Be 'x64'
        $l.DistDir | Should Be 'C:\ws\dist'
    }

    It 'builds arm64 by triple and keeps its packages apart' {
        $l = Get-CargoTargetLayout -Arch 'arm64' -TargetRoot $target -WorkspacePath $ws
        $l.IsCross | Should Be $true
        ($l.CargoArgs -join ' ') | Should Be '--target aarch64-pc-windows-msvc'
        $l.ArchTargetDir | Should Be 'C:\ct\aarch64-pc-windows-msvc'
        $l.ReleaseDir | Should Be 'C:\ct\aarch64-pc-windows-msvc\release'
        $l.PackageArch | Should Be 'arm64'
        $l.DistDir | Should Be 'C:\ws\dist\windows-arm64'
    }

    It 'takes the hub''s spellings and its environment default, and refuses an unknown arch' {
        (Get-CargoTargetLayout -Arch 'x64' -TargetRoot $target -WorkspacePath $ws).Arch | Should Be 'amd64'
        $saved = $env:WINDOWS_TARGET_ARCH
        try {
            $env:WINDOWS_TARGET_ARCH = 'arm64'
            (Get-CargoTargetLayout -TargetRoot $target -WorkspacePath $ws).Arch | Should Be 'arm64'
        } finally { $env:WINDOWS_TARGET_ARCH = $saved }
        $thrown = try { Get-CargoTargetLayout -Arch 'riscv64' -TargetRoot $target -WorkspacePath $ws | Out-Null; '' } catch { "$($_.Exception.Message)" }
        $thrown | Should Match 'Unsupported Windows target architecture'
    }
}
