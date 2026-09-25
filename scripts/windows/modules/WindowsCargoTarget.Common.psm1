#requires -Version 7.0

# PROJECT-LOCAL: where one Windows build of this repo lands. The arch facts themselves (the accepted
# spellings, cross or not, the Rust triple, what a package calls the arch) are the hub's
# WindowsTargetArch.Common and WindowsCrossBundle.Common, which the caller imports. amd64 is the build
# host and keeps cargo's plain target\release; arm64 is the cross build in the arm64 bundle, which
# cargo keys by triple. Both write their products to dist\windows-<x64|arm64>, the directory each
# Windows lane uploads whole.

Set-StrictMode -Version Latest

function Get-CargoTargetLayout {
    <#
    .SYNOPSIS
        The arch-dependent paths and names of one build: Arch (the hub's canonical name), IsCross,
        CargoArgs (the --target pair, empty on the host), ArchTargetDir (cargo's per-arch root, where the
        payloads and staging dirs go too), ReleaseDir, PackageArch (MSIX ProcessorArchitecture and
        `wix -arch`) and DistDir (dist\windows-<PackageArch>).
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
