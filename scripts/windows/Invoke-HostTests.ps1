#requires -Version 7.0
# Runs, on the runner HOST, the test binaries the Windows container builds but
# cannot start: wgpu's gles backend imports opengl32.dll at load time, which
# servercore does not ship (STATUS_DLL_NOT_FOUND before main). Invoke-DebugTests.ps1
# records one repo-relative path per line in target\host-tests\*.txt.
[CmdletBinding()]
param(
  # The workspace path INSIDE the build container. Test binaries embed it at compile time
  # (env!("CARGO_MANIFEST_DIR") -> C:\ws\crates\...), so their fixtures resolve on the host
  # only if that path reaches this tree (run 36038436509: four renderer tests could not load
  # C:\ws\crates\webgpu_renderer\tests\assets\*.gltf from a checkout at D:\ws).
  [string]$ContainerRoot = 'C:\ws'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$lists = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot 'target\host-tests') -Filter '*.txt' -File -ErrorAction SilentlyContinue)
if ($lists.Count -eq 0) {
  throw 'No test list under target\host-tests: Invoke-DebugTests.ps1 must run first, in the container.'
}

# A junction, not a copy: no admin needed, and the tree is the one the container built.
# An existing path that leads anywhere else is refused, never replaced.
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
