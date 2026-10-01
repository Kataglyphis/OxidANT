# Runs inside the Windows container; like Build-RustAll.ps1 it writes only to C:\ct / C:\ch (wcifs-safe).
#requires -Version 7.0

$ProgressPreference = 'SilentlyContinue'
# The driver stages this module into the scratch mount; see Build-RustAll.ps1.
Import-Module 'C:\host-scratch\WindowsContainerLog.Common.psm1' -Force
Start-ContainerLog -Path 'C:\host-scratch\in-container-test.log'

Write-ContainerLog "=== Rust container test run (debug): unit + integration + fuzz(proptest) + doc ==="
Write-ContainerLog "cpus: $env:NUMBER_OF_PROCESSORS"
[void](Invoke-ContainerLoggedCommand 'rustc -vV')

New-Item -ItemType Directory -Force -Path C:\ct, C:\ch | Out-Null
$env:CARGO_TARGET_DIR = 'C:\ct'
$env:CARGO_HOME = 'C:\ch'
Set-Location C:\ws-mnt

# Server Core lacks the opengl32.dll that wgpu's `gles` backend imports at load; see README.md § Tests.
Write-ContainerLog 'SKIPPING kataglyphis_webgpu_renderer: Server Core has no opengl32.dll (see comment in this script). Run it on a desktop Windows host.'

# The CI lane's set (Invoke-DebugTests.ps1): the features reach the cfg-gated capture and ORT-loader suites.
$code = Invoke-ContainerLoggedCommand 'cargo test --workspace --locked --exclude kataglyphis_webgpu_renderer --features kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'
if ($code -ne 0) { Write-ContainerLog "TESTS FAILED (exit $code)"; exit $code }
Write-ContainerLog 'ALL TESTS PASSED (kataglyphis_webgpu_renderer excluded -- see above)'
exit 0

