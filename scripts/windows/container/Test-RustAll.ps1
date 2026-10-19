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

# Server Core lacks the opengl32.dll that wgpu's `gles` backend imports at load; see docs/testing.md#on-windows-the-renderers-tests-run-outside-the-container
Write-ContainerLog 'SKIPPING kataglyphis_webgpu_renderer: Server Core has no opengl32.dll (see comment in this script). Run it on a desktop Windows host.'

# The CI lane's set (Invoke-DebugTests.ps1): the features reach the cfg-gated capture and ORT-loader suites.
$code = Invoke-ContainerLoggedCommand 'cargo test --workspace --locked --exclude kataglyphis_webgpu_renderer --features kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'
if ($code -ne 0) { Write-ContainerLog "TESTS FAILED (exit $code)"; exit $code }
Write-ContainerLog 'ALL TESTS PASSED (kataglyphis_webgpu_renderer excluded -- see above)'
exit 0
