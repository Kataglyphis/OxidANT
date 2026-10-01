#requires -Version 7.0
<#
.SYNOPSIS
  Builds the test binaries the x64 lane runs, for -TargetArch, and stages them for the arm64 run job (hub CON43).
.DESCRIPTION
  The four targets of Invoke-DebugTests.ps1, as `cargo test --no-run` for the target triple. They are built from the
  repo root (C:\ws in the container) like the x64 tests, so the CARGO_MANIFEST_DIR and CARGO_BIN_EXE paths they embed
  read C:\ws\...; the staged tree mirrors those paths, and the run job links C:\ws to it. Only the triple's own
  binaries are staged: host build scripts and proc macros would fail the arch gate.
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

# The x64 lane's four targets; the renderer's own GPU skip line is counted as a skip, not a pass.
$targets = @(
  @{ Args = @('--package', 'oxidant', '--lib') },
  @{ Args = @('--package', 'oxidant', '--test', 'fuzz_test') },
  @{ Args = @('--package', 'kataglyphis_cli', '--test', 'integration') },
  @{ Args = @('--package', 'kataglyphis_webgpu_renderer', '--lib'); Run = @('--nocapture'); SkipPattern = '^SKIP: no GPU adapter' }
)

$archDir = $layout.ArchTargetDir.TrimEnd('\') + '\'
$entries = [System.Collections.Generic.List[object]]::new()
$binaries = [System.Collections.Generic.List[string]]::new()
foreach ($t in $targets) {
  Write-Host "==> cargo test --no-run $($t.Args -join ' ') $($layout.CargoArgs -join ' ')"
  $messages = & cargo test --no-run --message-format=json @($t.Args) @($layout.CargoArgs)
  if ($LASTEXITCODE -ne 0) { throw "cargo test --no-run $($t.Args -join ' ') failed with exit code $LASTEXITCODE" }
  # Filter on `reason` first: only compiler-artifact messages carry the fields strict mode reads.
  $artifacts = @($messages | Where-Object { $_ -match '^\s*\{' } | ForEach-Object { $_ | ConvertFrom-Json } |
      Where-Object { $_.reason -eq 'compiler-artifact' -and $_.executable -and $_.executable.StartsWith($archDir, [StringComparison]::OrdinalIgnoreCase) })
  $tests = @($artifacts | Where-Object { $_.profile.test } | ForEach-Object executable | Select-Object -Unique)
  if ($tests.Count -eq 0) { throw "cargo test --no-run $($t.Args -join ' ') produced no test binary under $archDir" }
  foreach ($exe in $tests) {
    $entry = [ordered]@{ exe = [System.IO.Path]::GetRelativePath($repoRoot, $exe); kind = 'cargo' }
    if ($t.ContainsKey('Run')) { $entry['args'] = $t.Run }
    if ($t.ContainsKey('SkipPattern')) { $entry['skip_pattern'] = $t.SkipPattern }
    $entries.Add([pscustomobject]$entry)
    $binaries.Add($exe)
  }
  # The integration test starts kataglyphis_cli.exe by the path cargo embedded (CARGO_BIN_EXE_*).
  foreach ($bin in @($artifacts | Where-Object { -not $_.profile.test -and @($_.target.kind) -contains 'bin' } | ForEach-Object executable)) { $binaries.Add($bin) }
}

if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Recurse -Force }
$null = New-Item -ItemType Directory -Force -Path $Destination
foreach ($exe in @($binaries | Select-Object -Unique)) {
  $target = Join-Path $Destination ([System.IO.Path]::GetRelativePath($repoRoot, $exe))
  $null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target)
  Copy-Item -LiteralPath $exe -Destination $target
}
# Each directory gets its own closure: the loader looks beside the exe, not beside its siblings.
$closure = foreach ($dir in @($binaries | ForEach-Object { Split-Path -Parent $_ } | Select-Object -Unique)) {
  $staged = Join-Path $Destination ([System.IO.Path]::GetRelativePath($repoRoot, $dir))
  $exes = @($binaries | Where-Object { (Split-Path -Parent $_) -eq $dir } | Select-Object -Unique)
  Copy-PeImportClosure -Path $exes -SearchDirectory @(Get-ProductDllSearchPath -Arch $layout.Arch) -Destination $staged -Arch $layout.Arch
}
# What the renderer's tests read at run time through CARGO_MANIFEST_DIR: its source tree and its fixtures.
foreach ($rel in 'crates\webgpu_renderer\src', 'crates\webgpu_renderer\tests\assets') {
  Copy-Item -LiteralPath (Join-Path $repoRoot $rel) -Destination (Join-Path $Destination $rel) -Recurse -Force
}
Copy-Item -LiteralPath (Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\build\Invoke-StagedTests.ps1') -Destination $Destination
ConvertTo-Json -InputObject @($entries) -Depth 4 | Set-Content -LiteralPath (Join-Path $Destination 'tests.json') -Encoding utf8
Write-Host "Staged $($entries.Count) test binaries and $(@($binaries | Select-Object -Unique).Count - $entries.Count) program(s) in $Destination, with $(@($closure).Count) closure DLL(s)"
