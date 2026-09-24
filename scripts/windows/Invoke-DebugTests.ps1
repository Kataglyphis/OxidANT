param(
#requires -Version 7.0

  [string]$Package = 'oxidant'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $repoRoot

$testSteps = @(
  @{ Name = 'Unit tests'; Args = @('test', '--package', $Package, '--lib') },
  @{ Name = 'Integration tests'; Args = @('test', '--package', 'kataglyphis_cli', '--test', 'integration') },
  @{ Name = 'Fuzz tests'; Args = @('test', '--package', $Package, '--test', 'fuzz_test') }
)

foreach ($step in $testSteps) {
  Write-Host "==> $($step.Name)"
  & cargo @($step.Args)
  if ($LASTEXITCODE -ne 0) {
    throw "$($step.Name) failed with exit code $LASTEXITCODE."
  }
}

# The WebGPU renderer's lib tests are BUILT here and RUN on the runner host
# (Invoke-HostTests.ps1). Every renderer test binary links wgpu, whose gles
# backend imports opengl32.dll at load time, and servercore does not ship it:
# the process dies with STATUS_DLL_NOT_FOUND (0xc0000135) before main, so no
# "skip without an adapter" guard can run (lib binary: run 36019995362). The
# runner host is a desktop Windows Server, where the DLL exists (README.md).
Write-Host '==> WebGPU renderer lib tests: build here, run on the host'
$messages = & cargo test --package kataglyphis_webgpu_renderer --lib --no-run --message-format=json
if ($LASTEXITCODE -ne 0) {
  throw "Building the WebGPU renderer lib tests failed with exit code $LASTEXITCODE."
}
# compiler-artifact messages always carry target, profile and executable, so
# strict mode can read them once `reason` has picked those out.
$executables = @($messages | Where-Object { $_ -match '^\s*\{' } | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' } |
    Where-Object { $_.profile.test -and $_.target.name -eq 'kataglyphis_webgpu_renderer' -and $_.executable })
if ($executables.Count -ne 1) {
  throw "Expected one WebGPU renderer lib test executable; cargo reported $($executables.Count)."
}
$hostTestsDir = Join-Path $repoRoot 'target\host-tests'
$null = New-Item -ItemType Directory -Force -Path $hostTestsDir
$relative = [System.IO.Path]::GetRelativePath($repoRoot, $executables[0].executable)
Set-Content -LiteralPath (Join-Path $hostTestsDir 'webgpu-renderer-lib.txt') -Value $relative -Encoding utf8NoBOM
Write-Host "    recorded for the host: $relative"

