#requires -Version 7.0

# Project-local layout only; the arch facts are the hub's WindowsTargetArch/WindowsCrossBundle, imported by the caller.

Set-StrictMode -Version Latest

function Get-CargoTargetLayout {
    <#
    .SYNOPSIS
        One build's arch-dependent paths and names: Arch, IsCross, CargoArgs, ArchTargetDir, ReleaseDir, PackageArch, DistDir.
    #>
    [CmdletBinding()]
    param(
        [AllowEmptyString()][string] $Arch = '',
        [Parameter(Mandatory)][string] $TargetRoot,
        [Parameter(Mandatory)][string] $WorkspacePath
    )

    $resolved = Get-WindowsTargetArch -Arch $Arch
    $cross = Test-WindowsCrossTarget -Arch $resolved
    $triple = Get-RustTargetTriple -Arch $resolved
    $packageArch = Get-WindowsPackageArch -Arch $resolved
    $archTargetDir = if ($cross) { Join-Path $TargetRoot $triple } else { $TargetRoot }
    return [pscustomobject]@{
        Arch          = $resolved
        IsCross       = $cross
        CargoArgs     = [string[]]@(if ($cross) { '--target'; $triple })
        ArchTargetDir = $archTargetDir
        ReleaseDir    = Join-Path $archTargetDir 'release'
        PackageArch   = $packageArch
        DistDir       = Join-Path $WorkspacePath "dist\windows-$packageArch"
    }
}

function Get-CargoMsixVersion {
    <#
    .SYNOPSIS
        Cargo.toml's [workspace.package] version, else [package]'s, in MSIX's four parts: 2.3.4 becomes 2.3.4.0.
    #>
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$CargoToml)

    $section = ''
    $workspace = $null
    $package = $null
    foreach ($line in Get-Content -LiteralPath $CargoToml) {
        if ($line -match '^\s*\[(.+)\]\s*$') { $section = $Matches[1].Trim(); continue }
        if ($line -notmatch '^\s*version\s*=\s*"([^"]+)"') { continue }
        if ($section -eq 'workspace.package') { $workspace = $Matches[1] }
        elseif ($section -eq 'package' -and -not $package) { $package = $Matches[1] }
    }
    $version = if ($workspace) { $workspace } else { $package }
    if (-not $version) { throw "$CargoToml has no [workspace.package] or [package] version" }
    $parts = [System.Collections.Generic.List[string]]::new([string[]]$version.Split('-')[0].Split('.'))
    while ($parts.Count -lt 4) { $parts.Add('0') }
    return ($parts.GetRange(0, 4) -join '.')
}

Export-ModuleMember -Function Get-CargoTargetLayout, Get-CargoMsixVersion
