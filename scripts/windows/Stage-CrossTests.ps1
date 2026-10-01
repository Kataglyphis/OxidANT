#requires -Version 7.0
<#
.SYNOPSIS
  Builds the test binaries the x64 lane runs, for -TargetArch, and stages them for the arm64 run job (hub CON43).
.DESCRIPTION
  The whole workspace with the x64 lane's test features, renderer included, as `cargo test --no-run` for the target
  triple: the run job's windows-11-arm has the opengl32.dll the x64 container lacks. Built from the repo root (C:\ws in
  the container) like the x64 tests, so the CARGO_MANIFEST_DIR and CARGO_BIN_EXE paths they embed read C:\ws\...; the
  staged tree mirrors those paths, and the run job links C:\ws to it. Only the triple's own binaries are staged: host
  build scripts and proc macros would fail the arch gate.
#>
[CmdletBinding()]
param(
  [string]$TargetArch = 'arm64',
  # Empty: dist\windows-<arch>-tests beside the product.
  [string]$Destination = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')
Import-BuildModule @('WindowsTargetArch.Common', 'WindowsCrossBundle.Common', 'WindowsCargoTarget.Common')

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $repoRoot
$layout = Get-CargoTargetLayout -Arch $TargetArch -TargetRoot (Join-Path $repoRoot 'target') -WorkspacePath $repoRoot
if (-not $Destination) { $Destination = "$($layout.DistDir)-tests" }
# pkg-config refuses cross builds by default; the arm64 bundle's PKG_CONFIG_PATH is the target's.
if ($layout.IsCross) { $env:PKG_CONFIG_ALLOW_CROSS = '1' }

# Invoke-DebugTests.ps1's features.
$testFeatures = 'kataglyphis_media?/gstreamer,kataglyphis_inference?/onnxruntime'

$archDir = $layout.ArchTargetDir.TrimEnd('\') + '\'
Write-Host "==> cargo test --no-run --workspace --features $testFeatures $($layout.CargoArgs -join ' ')"
$messages = & cargo test --no-run --locked --message-format=json --workspace --features $testFeatures @($layout.CargoArgs)
if ($LASTEXITCODE -ne 0) { throw "cargo test --no-run --workspace failed with exit code $LASTEXITCODE" }
# Filter on `reason` first: only compiler-artifact messages carry the fields strict mode reads.
$artifacts = @($messages | Where-Object { $_ -match '^\s*\{' } | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.reason -eq 'compiler-artifact' -and $_.executable -and $_.executable.StartsWith($archDir, [StringComparison]::OrdinalIgnoreCase) })
$tests = @($artifacts | Where-Object { $_.profile.test })
if ($tests.Count -eq 0) { throw "cargo test --no-run --workspace produced no test binary under $archDir" }
$isRenderer = { param($a) [System.IO.Path]::GetRelativePath($repoRoot, $a.manifest_path) -eq 'crates\webgpu_renderer\Cargo.toml' }
$rendererTests = @($tests | Where-Object { & $isRenderer $_ })
$expectedRenderer = 1 + @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'crates\webgpu_renderer\tests') -Filter '*.rs' -File).Count
if ($rendererTests.Count -ne $expectedRenderer) {
  throw "Expected $expectedRenderer WebGPU renderer test binaries (the lib and one per tests\*.rs); cargo reported $($rendererTests.Count)."
}

# The renderer's GPU skip line counts as a skip; serial, as WARP on windows-11-arm killed a parallel headless.exe (BACKLOG).
$entries = foreach ($test in $tests) {
  $entry = [ordered]@{ exe = [System.IO.Path]::GetRelativePath($repoRoot, $test.executable); kind = 'cargo' }
  if (& $isRenderer $test) {
    $entry['args'] = @('--nocapture', '--test-threads=1')
    $entry['skip_pattern'] = '^SKIP: no GPU adapter'
  }
  [pscustomobject]$entry
}
# CARGO_BIN_EXE_* reaches only a package's own integration tests (the CLI's starts kataglyphis_cli.exe by that path).
$withIntegrationTests = @($tests | Where-Object { @($_.target.kind) -contains 'test' } | ForEach-Object manifest_path | Select-Object -Unique)
$programs = @($artifacts | Where-Object { -not $_.profile.test -and @($_.target.kind) -contains 'bin' -and $withIntegrationTests -contains $_.manifest_path })
$binaries = @(@($tests | ForEach-Object executable) + @($programs | ForEach-Object executable) | Select-Object -Unique)

if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Recurse -Force }
$null = New-Item -ItemType Directory -Force -Path $Destination
foreach ($exe in $binaries) {
  $target = Join-Path $Destination ([System.IO.Path]::GetRelativePath($repoRoot, $exe))
  $null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target)
  Copy-Item -LiteralPath $exe -Destination $target
}

# The capture test loads its elements by name, so the product's plugin set goes where bundled_plugin_dir() looks.
$config = Import-PowerShellDataFile -Path (Join-Path $PSScriptRoot 'Build-Windows.config.psd1')
$gstPluginDir = if ($env:GSTREAMER_PLUGIN_DIR) { $env:GSTREAMER_PLUGIN_DIR } else { $config.Build.GStreamerPluginDir }
$plugins = @($config.Build.GStreamerPlugins | ForEach-Object { Join-Path $gstPluginDir "$_.dll" })
$missing = @($plugins | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) })
if ($missing.Count -gt 0) { throw "GStreamer plugins missing from $gstPluginDir`: $($missing -join ', ')" }

# Each directory gets its own closure: the loader looks beside the exe, not beside its siblings.
$search = @(Get-ProductDllSearchPath -Arch $layout.Arch)
$closure = foreach ($dir in @($binaries | ForEach-Object { Split-Path -Parent $_ } | Select-Object -Unique)) {
  $staged = Join-Path $Destination ([System.IO.Path]::GetRelativePath($repoRoot, $dir))
  $exes = @($binaries | Where-Object { (Split-Path -Parent $_) -eq $dir })
  $seeds = $exes
  if (@($exes | Where-Object { @(Get-PeImportNames -Path $_) -contains 'gstreamer-1.0-0.dll' }).Count -gt 0) {
    $pluginTarget = Join-Path $staged 'lib\gstreamer-1.0'
    $null = New-Item -ItemType Directory -Force -Path $pluginTarget
    Copy-Item -LiteralPath $plugins -Destination $pluginTarget
    $seeds = $exes + @($plugins)
  }
  Copy-PeImportClosure -Path $seeds -SearchDirectory $search -Destination $staged -Arch $layout.Arch
}
# What the renderer's tests read at run time through CARGO_MANIFEST_DIR: its source tree and its fixtures.
foreach ($rel in 'crates\webgpu_renderer\src', 'crates\webgpu_renderer\tests\assets') {
  Copy-Item -LiteralPath (Join-Path $repoRoot $rel) -Destination (Join-Path $Destination $rel) -Recurse -Force
}
Copy-Item -LiteralPath (Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\build\Invoke-StagedTests.ps1') -Destination $Destination
ConvertTo-Json -InputObject @($entries) -Depth 4 | Set-Content -LiteralPath (Join-Path $Destination 'tests.json') -Encoding utf8
Write-Host "Staged $(@($entries).Count) test binaries and $($binaries.Count - @($entries).Count) program(s) in $Destination, with $(@($closure).Count) closure DLL(s)"
