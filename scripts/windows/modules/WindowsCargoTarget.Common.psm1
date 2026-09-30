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

Export-ModuleMember -Function Get-CargoTargetLayout
