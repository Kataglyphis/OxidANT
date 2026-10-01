#requires -Version 7.0
# The x64 lane's tests in the container; the renderer's are only built here, for Invoke-HostTests.ps1 to run.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $repoRoot

# The Linux test step's features (scripts/linux/ci-container-steps.sh TEST_FEATURES), which Stage-CrossTests.ps1 builds too.
$testFeatures = 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'

Write-Host '==> Workspace tests, the WebGPU renderer excluded'
& cargo test --workspace --locked --exclude kataglyphis_webgpu_renderer --features $testFeatures
if ($LASTEXITCODE -ne 0) {
  throw "Workspace tests failed with exit code $LASTEXITCODE."
}

# Built here, run by Invoke-HostTests.ps1: servercore lacks the opengl32.dll wgpu imports at load.
Write-Host '==> WebGPU renderer tests: build here, run on the host'
$messages = & cargo test --locked --package kataglyphis_webgpu_renderer --lib --tests --no-run --message-format=json
if ($LASTEXITCODE -ne 0) {
  throw "Building the WebGPU renderer tests failed with exit code $LASTEXITCODE."
}
# Filter on `reason` first: only compiler-artifact messages carry the fields strict mode reads.
$executables = @($messages | Where-Object { $_ -match '^\s*\{' } | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' } |
    Where-Object { $_.profile.test -and $_.executable } | ForEach-Object executable | Select-Object -Unique)
# The lib and one binary per tests\*.rs, so a target cargo stopped building cannot drop out unseen.
$expected = 1 + @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates\webgpu_renderer\tests') -Filter '*.rs' -File).Count
if ($executables.Count -ne $expected) {
  throw "Expected $expected WebGPU renderer test executables (the lib and one per tests\*.rs); cargo reported $($executables.Count)."
}

# The hub's Invoke-StagedTests.ps1 reads this manifest on the host, as it reads the arm64 lane's on windows-11-arm.
$hostTestsDir = Join-Path $repoRoot 'target\host-tests'
if (Test-Path -LiteralPath $hostTestsDir) { Remove-Item -LiteralPath $hostTestsDir -Recurse -Force }
$null = New-Item -ItemType Directory -Force -Path $hostTestsDir
$entries = foreach ($exe in $executables) {
  [pscustomobject][ordered]@{
    exe = [System.IO.Path]::GetRelativePath($hostTestsDir, $exe)
    kind = 'cargo'
    args = @('--nocapture')
    skip_pattern = '^SKIP: no GPU adapter'
  }
}
ConvertTo-Json -InputObject @($entries) -Depth 4 | Set-Content -LiteralPath (Join-Path $hostTestsDir 'tests.json') -Encoding utf8NoBOM
Write-Host "    recorded $($executables.Count) test executables for the host in target\host-tests\tests.json"
