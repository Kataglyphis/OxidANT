#requires -Version 7.0
# Runs on the host the test binaries listed in target\host-tests\*.txt, which servercore cannot load.
[CmdletBinding()]
param(
  # The container's workspace path, which test binaries embed via CARGO_MANIFEST_DIR for fixtures.
  [string]$ContainerRoot = 'C:\ws'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$lists = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'target\host-tests') -Filter '*.txt' -File -ErrorAction SilentlyContinue)
if ($lists.Count -eq 0) {
  throw 'No test list under target\host-tests: Invoke-DebugTests.ps1 must run first, in the container.'
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
