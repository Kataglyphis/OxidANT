# Runs inside the Windows container; like Build-RustAll.ps1 it builds into C:\ct / C:\ch, which the reused container keeps.
#requires -Version 7.0
[CmdletBinding()]
param(
    # The runner's workspace path inside the container, as for Build-RustAll.ps1.
    [string]$Workspace = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path,
    # Empty: <Workspace>\logs, which the driver streams back to the host.
    [string]$LogDir = ''
)

$ProgressPreference = 'SilentlyContinue'
. (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
Import-BuildModule 'WindowsContainerLog.Common'
if (-not $LogDir) { $LogDir = Join-Path $Workspace 'logs' }
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
Start-ContainerLog -Path (Join-Path $LogDir 'in-container-test.log')

Write-ContainerLog "=== Rust container test run (debug): unit + integration + fuzz(proptest) + doc ==="
Write-ContainerLog "cpus: $env:NUMBER_OF_PROCESSORS"
[void](Invoke-ContainerLoggedCommand 'rustc -vV')

New-Item -ItemType Directory -Force -Path C:\ct, C:\ch | Out-Null
$env:CARGO_TARGET_DIR = 'C:\ct'
$env:CARGO_HOME = 'C:\ch'
Set-Location $Workspace

# Unset, cc-rs takes MSVC's cl; every build compiles C and C++ with the image's clang-cl, as the CI lanes do.
$clangCl = Get-Command clang-cl -ErrorAction SilentlyContinue
if (-not $clangCl) { Write-ContainerLog 'FATAL: no clang-cl on PATH'; exit 1 }
$env:CC = 'clang-cl'
$env:CXX = 'clang-cl'
Write-ContainerLog "C and C++ through $($clangCl.Source) (CC, CXX)"

# Without the model the real-model tests pass by skipping; 0 opts out.
if (-not $env:KATAGLYPHIS_REQUIRE_MODEL) { $env:KATAGLYPHIS_REQUIRE_MODEL = '1' }
# The image carries the chain GenAI under ONNX_GENAI_ROOT, so the text suites must load it rather than skip.
if (-not $env:KATAGLYPHIS_REQUIRE_GENAI) { $env:KATAGLYPHIS_REQUIRE_GENAI = '1' }
& (Join-Path $Workspace 'scripts\windows\Get-Models.ps1')
if (-not $?) { Write-ContainerLog 'FATAL: Get-Models.ps1 failed'; exit 1 }

# Server Core lacks the opengl32.dll that wgpu's `gles` backend imports at load; see docs/testing.md#on-windows-the-renderers-tests-run-outside-the-container
Write-ContainerLog 'SKIPPING kataglyphis_webgpu_renderer: Server Core has no opengl32.dll (see comment in this script). Run it on a desktop Windows host.'

# The CI lane's set (Invoke-DebugTests.ps1).
$code = Invoke-ContainerLoggedCommand 'cargo test --workspace --locked --exclude kataglyphis_webgpu_renderer'
if ($code -ne 0) { Write-ContainerLog "TESTS FAILED (exit $code)"; exit $code }
Write-ContainerLog 'ALL TESTS PASSED (kataglyphis_webgpu_renderer excluded -- see above)'
exit 0
