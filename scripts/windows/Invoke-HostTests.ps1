#requires -Version 7.0
# Runs on the host the test binaries target\host-tests\tests.json lists, which servercore cannot load.
[CmdletBinding()]
param(
  # The container's workspace path, which test binaries embed via CARGO_MANIFEST_DIR for fixtures.
  [string]$ContainerRoot = 'C:\ws'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$manifest = Join-Path $repoRoot 'target\host-tests\tests.json'
if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
  throw 'No target\host-tests\tests.json: Invoke-DebugTests.ps1 must run first, in the container.'
}

# A junction needs no admin; an existing path leading elsewhere is refused, never replaced.
if ($ContainerRoot -and ($ContainerRoot.TrimEnd('\') -ne $repoRoot.TrimEnd('\'))) {
  $existing = Get-Item -LiteralPath $ContainerRoot -Force -ErrorAction SilentlyContinue
  if (-not $existing) {
    $null = New-Item -ItemType Junction -Path $ContainerRoot -Target $repoRoot
    Write-Host "Linked $ContainerRoot -> $repoRoot (the test binaries' compile-time paths)"
  } elseif ($existing.LinkType -ne 'Junction' -or (@($existing.Target)[0].TrimEnd('\') -ne $repoRoot.TrimEnd('\'))) {
    throw "$ContainerRoot exists and is not a junction to $repoRoot; the test binaries' compile-time paths would read another tree."
  }
}

# The arm64 lane's runner too, so both Windows lanes count passes, failures and self-skips the same way.
$runner = Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\build\Invoke-StagedTests.ps1'
$output = @(& $runner -Manifest $manifest)
$output | ForEach-Object { Write-Host $_ }
$verdict = @($output | Select-String -Pattern '^TESTS: passed=(\d+) failed=(\d+) skipped=(\d+)\s*$')
if ($verdict.Count -ne 1) {
  throw "Invoke-StagedTests.ps1 printed $($verdict.Count) 'TESTS: passed=<n> failed=<n> skipped=<n>' lines; exactly one is the verdict."
}
$passed, $failed, $skipped = $verdict[0].Matches[0].Groups[1..3].Value | ForEach-Object { [int]$_ }
if ($failed -gt 0) { throw "$failed host test(s) failed ($passed passed, $skipped skipped)." }
if ($passed -lt 1) { throw 'No host test passed; a run with nothing tested is not a test verdict.' }
