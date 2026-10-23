#requires -Version 7.0
# OxidANT owns the inputs; the hub owns the compiler and emit checks.
param([switch]$Check)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsSlang.Common')
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$sourceRoot = Join-Path $repoRoot 'shaders'
$destination = if ($Check) { Join-Path ([IO.Path]::GetTempPath()) ('oxidant-shaders-' + [guid]::NewGuid()) } else { $repoRoot }
try {
    Invoke-SlangShaderCompile -ManifestPath (Join-Path $sourceRoot 'shader-manifest.json') `
        -SourceRoot $sourceRoot -DestinationRoot $destination
    if ($Check) {
        & python (Join-Path $repoRoot 'scripts/check-shader-output.py') $destination
        if ($LASTEXITCODE -ne 0) { throw 'Generated WGSL differs from its owned Slang source' }
    }
} finally {
    if ($Check -and (Test-Path -LiteralPath $destination)) {
        $resolved = [IO.Path]::GetFullPath($destination)
        $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if (-not $resolved.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Refusing to remove a non-temporary shader tree' }
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
