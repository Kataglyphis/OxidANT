#requires -Version 7.0

# Written for Pester 3.4.0: no BeforeAll outside Describe, dash-less Should.

Describe 'WindowsCargoTarget.Common' {

    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsTargetArch.Common'
    Import-BuildModule 'WindowsCrossBundle.Common'
    Import-BuildModule 'WindowsCargoTarget.Common'

    $target = 'C:\ct'
    $ws = 'C:\ws'

    function Get-Layout([string]$Arch) { Get-CargoTargetLayout -Arch $Arch -TargetRoot $target -WorkspacePath $ws }

    # Should writes to the pipeline in Pester 3.4, so this cannot also return the layout.
    function Assert-CargoLayout($l, [bool]$IsCross, [string]$ArchTargetDir, [string]$PackageArch) {
        $l.IsCross | Should Be $IsCross
        $l.ArchTargetDir | Should Be $ArchTargetDir
        $l.ReleaseDir | Should Be "$ArchTargetDir\release"
        $l.PackageArch | Should Be $PackageArch
        $l.DistDir | Should Be "C:\ws\dist\windows-$PackageArch"
    }

    It 'keeps the host build in target\release, and names its packages x64 under dist\windows-x64' {
        $l = Get-Layout 'amd64'
        Assert-CargoLayout $l -IsCross $false -ArchTargetDir 'C:\ct' -PackageArch 'x64'
        @($l.CargoArgs).Count | Should Be 0
    }

    It 'builds arm64 by triple and keeps its packages apart' {
        $l = Get-Layout 'arm64'
        Assert-CargoLayout $l -IsCross $true -ArchTargetDir 'C:\ct\aarch64-pc-windows-msvc' -PackageArch 'arm64'
        ($l.CargoArgs -join ' ') | Should Be '--target aarch64-pc-windows-msvc'
    }

    It 'takes the hub''s spellings and its environment default, and refuses an unknown arch' {
        (Get-Layout 'x64').Arch | Should Be 'amd64'
        $saved = $env:WINDOWS_TARGET_ARCH
        try {
            $env:WINDOWS_TARGET_ARCH = 'arm64'
            (Get-CargoTargetLayout -TargetRoot $target -WorkspacePath $ws).Arch | Should Be 'arm64'
        } finally { $env:WINDOWS_TARGET_ARCH = $saved }
        $thrown = try { Get-CargoTargetLayout -Arch 'riscv64' -TargetRoot $target -WorkspacePath $ws | Out-Null; '' } catch { "$($_.Exception.Message)" }
        $thrown | Should Match 'Unsupported Windows target architecture'
    }
}
