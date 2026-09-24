#requires -Version 7.0
# Runs, on the runner HOST, the test binaries the Windows container builds but
# cannot start: wgpu's gles backend imports opengl32.dll at load time, which
# servercore does not ship (STATUS_DLL_NOT_FOUND before main). Invoke-DebugTests.ps1
# records one repo-relative path per line in target\host-tests\*.txt.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$lists = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'target\host-tests') -Filter '*.txt' -File -ErrorAction SilentlyContinue)
if ($lists.Count -eq 0) {
  throw 'No test list under target\host-tests: Invoke-DebugTests.ps1 must run first, in the container.'
}

foreach ($list in $lists) {
  $entries = @(Get-Content -LiteralPath $list.FullName | ForEach-Object { $_.Trim() } | Where-Object { $_ })
  if ($entries.Count -eq 0) {
    throw "$($list.Name) names no test binary."
  }
  foreach ($relative in $entries) {
    $exe = Join-Path $repoRoot $relative
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
      throw "$($list.Name) names $relative, which does not exist under $repoRoot."
    }
    Write-Host "==> $relative"
    & $exe
    if ($LASTEXITCODE -ne 0) {
      throw "$relative failed with exit code $LASTEXITCODE."
    }
  }
}
