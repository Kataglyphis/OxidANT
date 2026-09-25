#requires -Version 7.0

# PROJECT-LOCAL: where one Windows build of this repo lands and what its packages call the arch. The arch
# facts themselves (the accepted spellings, cross or not, the Rust triple) are the hub's
# WindowsTargetArch.Common, which the caller imports. amd64 is the build host and keeps cargo's plain
# target\release, so the x64 lane's paths do not move; arm64 is the cross build in the arm64 bundle
# (windows-arm64-cross.yml), which cargo keys by triple.

Set-StrictMode -Version Latest

$script:PackageArch = @{ amd64 = 'x64'; arm64 = 'arm64' }

function Get-CargoTargetLayout {
    <#
    .SYNOPSIS
        The arch-dependent paths and names of one build: Arch (the hub's canonical name), IsCross,
        CargoArgs (the --target pair, empty on the host), ArchTargetDir (cargo's per-arch root, where the
        payloads and staging dirs go too), ReleaseDir, PackageArch (MSIX ProcessorArchitecture and
        `wix -arch`) and DistDir (dist on the host, dist\windows-<arch> cross).
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
    $archTargetDir = if ($cross) { Join-Path $TargetRoot $triple } else { $TargetRoot }
    return [pscustomobject]@{
        Arch          = $resolved
        IsCross       = $cross
        CargoArgs     = [string[]]@(if ($cross) { '--target'; $triple })
        ArchTargetDir = $archTargetDir
        ReleaseDir    = Join-Path $archTargetDir 'release'
        PackageArch   = $script:PackageArch[$resolved]
        DistDir       = if ($cross) { Join-Path $WorkspacePath "dist\windows-$resolved" } else { Join-Path $WorkspacePath 'dist' }
    }
}

Export-ModuleMember -Function Get-CargoTargetLayout
