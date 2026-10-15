#requires -Version 7.0

# Written for Pester 3.4.0: no BeforeAll outside Describe, dash-less Should.

Describe 'WindowsCargoTarget.Common' {

    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsTargetArch.Common'
    Import-BuildModule 'WindowsCrossBundle.Common'
    Import-BuildModule 'WindowsCargoTarget.Common'

    $target = 'C:\ct'
    $ws = 'C:\ws'

    It 'keeps the host build in target\release, and names its packages x64 under dist\windows-x64' {
        $l = Get-CargoTargetLayout -Arch 'amd64' -TargetRoot $target -WorkspacePath $ws
        $l.IsCross | Should Be $false
        @($l.CargoArgs).Count | Should Be 0
        $l.ArchTargetDir | Should Be 'C:\ct'
        $l.ReleaseDir | Should Be 'C:\ct\release'
        $l.PackageArch | Should Be 'x64'
        $l.DistDir | Should Be 'C:\ws\dist\windows-x64'
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

    It 'takes the MSIX version from Cargo.toml''s [workspace.package], in four parts' {
        $toml = Join-Path $TestDrive 'ws.toml'
        Set-Content -LiteralPath $toml -Value @('[package]', 'name = "oxidant"', 'version.workspace = true', '', '[workspace.package]', 'version = "2.3.4"', '', '[dependencies]', 'serde = { version = "1.0.200" }')
        Get-CargoMsixVersion -CargoToml $toml | Should Be '2.3.4.0'
    }

    It 'falls back to [package]''s version, and never takes a dependency table''s' {
        $toml = Join-Path $TestDrive 'one.toml'
        Set-Content -LiteralPath $toml -Value @('[package]', 'name = "one"', 'version = "0.4.1-beta"', '', '[dependencies.serde]', 'version = "1.0.200"')
        Get-CargoMsixVersion -CargoToml $toml | Should Be '0.4.1.0'
    }

    It 'refuses a Cargo.toml with no version of its own' {
        $toml = Join-Path $TestDrive 'none.toml'
        Set-Content -LiteralPath $toml -Value @('[package]', 'name = "none"', 'version.workspace = true')
        $thrown = try { Get-CargoMsixVersion -CargoToml $toml | Out-Null; '' } catch { "$($_.Exception.Message)" }
        $thrown | Should Match 'no \[workspace.package\] or \[package\] version'
    }

    It 'reads this repo''s own Cargo.toml, the one version source' {
        Get-CargoMsixVersion -CargoToml (Join-Path $PSScriptRoot '..\..\..\Cargo.toml') | Should Match '^\d+\.\d+\.\d+\.0$'
    }
}
