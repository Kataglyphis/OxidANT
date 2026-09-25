#requires -Version 7.0

# PROJECT-LOCAL glue for the release payloads (zip, MSIX, MSI): stage the image's chain ONNX Runtime
# beside the exe and prove each payload with ANTfrastructure's G6 census (WindowsOrtProvenance.Common),
# which the caller imports. Owner rule 2026-09-23 (third_party/ANTfrastructure/docs/onnxruntime-single-source.md).
# Every provenance verdict is G6's. NOT covered: which copy a process loads beyond G6's modelled loader order.

Set-StrictMode -Version Latest

$script:OrtRuntimeFiles = @('onnxruntime.dll', 'onnxruntime_providers_shared.dll', 'DirectML.dll')
$script:OrtFamily = @('onnxruntime*.dll', 'DirectML.dll')

function Get-OrtCensusRequirement {
    <#
    .SYNOPSIS
        The error for a hub pin that lacks G6: which hub commit is needed, and where the pin is.
    #>
    [CmdletBinding()]
    [OutputType([string])]
    param([AllowNull()][object] $Cause = $null)

    $hub = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure'))
    $at = try { "$(& git -C $hub rev-parse --short HEAD 2>$null)".Trim() } catch { '' }
    return ("ONNX Runtime proof needs ANTfrastructure's G6 census (windows/scripts/modules/" +
        "WindowsOrtProvenance.Common.psm1, Test-OrtProvenanceTree), added by the hub's ORT single-source " +
        "commit of 2026-09-23 (third_party/ANTfrastructure/docs/onnxruntime-single-source.md). third_party/ANTfrastructure is at " +
        "$(if ($at) { $at } else { 'an unknown commit' }): move the gitlink to that commit or later." +
        $(if ($Cause) { " ($Cause)" } else { '' }))
}

function Assert-OrtCensusCommand {
    foreach ($cmd in 'Test-OrtProvenanceTree', 'Get-OrtBinaryFact', 'Get-OrtTreeFact', 'Get-OrtChainPrefix', 'Test-OrtInstanceName') {
        if (-not (Get-Command -Name $cmd -ErrorAction SilentlyContinue)) { throw (Get-OrtCensusRequirement -Cause "$cmd is not loaded") }
    }
}

function Test-OrtFamilyName {
    param([Parameter(Mandatory)][string] $Name)
    return @($script:OrtFamily | Where-Object { $Name -like $_ }).Count -gt 0
}

function Get-OrtFamilyFile {
    [CmdletBinding()]
    [OutputType([System.IO.FileInfo[]])]
    param([Parameter(Mandatory)][string] $Directory)
    return @(Get-ChildItem -LiteralPath $Directory -File -ErrorAction SilentlyContinue | Where-Object { Test-OrtFamilyName -Name $_.Name })
}

function Test-ExeLoadsOrt {
    <#
    .SYNOPSIS
        True when G6 counts the exe as an ORT consumer: it names the ORT ABI (OrtGetApiBase, which ort's
        load-dynamic resolves) or imports an ORT DLL.
    #>
    [CmdletBinding()]
    [OutputType([bool])]
    param([Parameter(Mandatory)][string] $Path)

    Assert-OrtCensusCommand
    $fact = Get-OrtBinaryFact -Path $Path
    if ($fact.Error) { throw "Cannot read $Path to decide whether it loads ONNX Runtime: $($fact.Error)" }
    return (@($fact.Abi).Count -gt 0) -or (@($fact.Imports | Where-Object { Test-OrtInstanceName -Name $_ }).Count -gt 0)
}

function Test-PayloadLoadsOrt {
    <#
    .SYNOPSIS
        True when the exe, or any non-ORT DLL a payload ships (beside it, or under an -IncludeDirectory
        tree), is a G6 ORT consumer (Test-ExeLoadsOrt): a consumer DLL beside a plain exe still loads
        ORT, System32's if none ships.
    #>
    [CmdletBinding()]
    [OutputType([bool])]
    param(
        [Parameter(Mandatory)][string] $ExePath,
        [string[]] $IncludeDirectory = @()
    )

    if (Test-ExeLoadsOrt -Path $ExePath) { return $true }
    $exeDir = Split-Path $ExePath -Parent
    $dlls = @(Get-ChildItem -LiteralPath $exeDir -Filter '*.dll' -File) +
        @($IncludeDirectory | ForEach-Object { Get-ChildItem -LiteralPath (Join-Path $exeDir $_) -Filter '*.dll' -File -Recurse -ErrorAction SilentlyContinue })
    $dlls = @($dlls | Where-Object { -not (Test-OrtFamilyName -Name $_.Name) })
    return @($dlls | Where-Object { Test-ExeLoadsOrt -Path $_.FullName }).Count -gt 0
}

function Copy-ChainOrtBeside {
    <#
    .SYNOPSIS
        Replaces every ORT-family DLL in Destination with the chain's from OnnxRoot\bin.
        Provenance is not judged here: New-OrtProvenPayload (G6) does that.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string] $OnnxRoot,
        [Parameter(Mandatory)][string] $Destination
    )

    if ([string]::IsNullOrWhiteSpace($OnnxRoot)) {
        throw 'The exe loads ONNX Runtime but ONNX_ROOT is unset: package inside the family Windows image, whose chain-built ORT is the only one allowed.'
    }
    $bin = Join-Path $OnnxRoot 'bin'
    if (-not (Test-Path -LiteralPath (Join-Path $bin 'onnxruntime.dll') -PathType Leaf)) {
        throw "No chain ONNX Runtime at $bin\onnxruntime.dll: ONNX_ROOT must be the image's chain install."
    }
    if (-not $PSCmdlet.ShouldProcess($Destination, 'stage chain ONNX Runtime')) { return }
    Get-OrtFamilyFile -Directory $Destination | Remove-Item -Force
    foreach ($name in $script:OrtRuntimeFiles) {
        $src = Join-Path $bin $name
        if (Test-Path -LiteralPath $src -PathType Leaf) { Copy-Item -LiteralPath $src -Destination $Destination -Force }
    }
}

function Get-OrtPayloadFinding {
    # What G6 does not grade: an ORT-family name outside the runtime set, and DirectML.dll's bytes.
    param([Parameter(Mandatory)][string] $Directory)
    $chainBin = Join-Path (Get-OrtChainPrefix) 'bin'
    if (-not (Test-Path -LiteralPath (Join-Path $Directory 'onnxruntime.dll') -PathType Leaf)) {
        "MISSING $Directory\onnxruntime.dll: without it a client host loads System32's Windows ML copy"
    }
    foreach ($file in (Get-OrtFamilyFile -Directory $Directory)) {
        if ($script:OrtRuntimeFiles -notcontains $file.Name) { "STRAY $($file.FullName) is not a chain runtime file"; continue }
        if (Test-OrtInstanceName -Name $file.Name) { continue }
        $chain = Join-Path $chainBin $file.Name
        $same = (Test-Path -LiteralPath $chain -PathType Leaf) -and
            (Get-FileHash -LiteralPath $chain -Algorithm SHA256).Hash -eq (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
        if (-not $same) { "CHANGED $($file.FullName) is not the chain's $chain" }
    }
}

function New-OrtProvenPayload {
    <#
    .SYNOPSIS
        Copies the exe and the DLLs beside it into a fresh Destination and proves that payload: when the
        exe or a shipped DLL loads ONNX Runtime (Test-PayloadLoadsOrt), G6 over it must pass; otherwise it
        carries no ORT at all. Packages ship from Destination, so the bytes proved are the bytes shipped.
    .PARAMETER IncludeDirectory
        Subdirectories beside the exe that ship whole with it (lib, for its GStreamer plugins). Copied
        before the proof, so G6 grades them with everything else; a missing one is skipped.
    #>
    [CmdletBinding(SupportsShouldProcess)]
    param(
        [Parameter(Mandatory)][string] $ExePath,
        [Parameter(Mandatory)][string] $Destination,
        [string[]] $IncludeDirectory = @()
    )

    Assert-OrtCensusCommand
    if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) { throw "Expected executable not found: $ExePath" }
    if (-not $PSCmdlet.ShouldProcess($Destination, 'build and prove the release payload')) { return }
    $loadsOrt = Test-PayloadLoadsOrt -ExePath $ExePath -IncludeDirectory $IncludeDirectory
    if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Recurse -Force }
    New-Item -ItemType Directory -Force -Path $Destination | Out-Null
    Copy-Item -LiteralPath $ExePath -Destination $Destination
    $exeDir = Split-Path $ExePath -Parent
    Get-ChildItem -LiteralPath $exeDir -Filter '*.dll' -File |
        Where-Object { $loadsOrt -or -not (Test-OrtFamilyName -Name $_.Name) } |
        ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $Destination }
    $included = [string[]]@(foreach ($name in @($IncludeDirectory | Select-Object -Unique)) {
            $source = Join-Path $exeDir $name
            if (-not (Test-Path -LiteralPath $source -PathType Container)) { continue }
            $target = Join-Path $Destination $name
            New-Item -ItemType Directory -Force -Path (Split-Path $target -Parent) | Out-Null
            Copy-Item -LiteralPath $source -Destination $target -Recurse
            $target
        })

    $findings = [System.Collections.Generic.List[string]]::new()
    if ($loadsOrt) {
        foreach ($f in @(Get-OrtPayloadFinding -Directory $Destination)) { $findings.Add($f) }
        $census = Test-OrtProvenanceTree -Root $Destination -PassThru
        foreach ($f in @($census.Findings | Where-Object Fatal)) { $findings.Add("$($f.Verdict) $($f.Path) -- $($f.Detail)") }
    } else {
        foreach ($f in @(Get-OrtTreeFact -ContentRoot @($Destination) | Where-Object IsInstance)) {
            $findings.Add("UNEXPECTED $($f.Path) is an ONNX Runtime binary beside an exe that does not load one")
        }
    }
    if ($findings.Count -gt 0) {
        throw ("The payload in $Destination does not carry exactly the image's chain ONNX Runtime (G6):" +
            [Environment]::NewLine + '  ' + ($findings -join ([Environment]::NewLine + '  ')))
    }
    $exe = Join-Path $Destination (Split-Path $ExePath -Leaf)
    return [pscustomobject]@{
        Directory = $Destination
        Exe       = $exe
        Dlls      = [string[]]@(Get-ChildItem -LiteralPath $Destination -Filter '*.dll' -File | ForEach-Object FullName)
        Included  = $included
        OrtDlls   = [string[]]@(Get-OrtFamilyFile -Directory $Destination | ForEach-Object FullName)
        LoadsOrt  = $loadsOrt
    }
}

Export-ModuleMember -Function Get-OrtCensusRequirement, Get-OrtFamilyFile, Test-ExeLoadsOrt, Test-PayloadLoadsOrt, Copy-ChainOrtBeside, New-OrtProvenPayload
