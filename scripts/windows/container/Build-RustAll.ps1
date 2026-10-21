# In-container debug/profile/release build into C:\ct, C:\ch, which the reused container keeps between runs.
#requires -Version 7.0
[CmdletBinding()]
param(
    # The runner's workspace path inside the container (Invoke-RepoContainerBuild.ps1 streams the sources there).
    [string]$Workspace = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path,
    # Empty: <Workspace>\logs, which the driver streams back to the host.
    [string]$LogDir = ''
)

# EAP stays 'Continue' and $LASTEXITCODE is checked by hand: native stderr handling varies by version.
$ProgressPreference = 'SilentlyContinue'

. (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
Import-BuildModule 'WindowsContainerLog.Common'

# Log to a file too: the docker CLI pipe drops output intermittently on this host.
if (-not $LogDir) { $LogDir = Join-Path $Workspace 'logs' }
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
Start-ContainerLog -Path (Join-Path $LogDir 'in-container-build.log')

Write-ContainerLog "=== Rust container build: debug / profile / release ==="
Write-ContainerLog "cpus: $env:NUMBER_OF_PROCESSORS"
[void](Invoke-ContainerLoggedCommand 'rustc -vV')
if ((Invoke-ContainerLoggedCommand 'cargo --version') -ne 0) { Write-ContainerLog 'FATAL: cargo not usable'; exit 1 }

New-Item -ItemType Directory -Force -Path C:\ct, C:\ch | Out-Null
$env:CARGO_TARGET_DIR = 'C:\ct'
$env:CARGO_HOME = 'C:\ch'

# Unset, cc-rs takes MSVC's cl; every build compiles C and C++ with the image's clang-cl, as the CI lanes do.
$clangCl = Get-Command clang-cl -ErrorAction SilentlyContinue
if (-not $clangCl) { Write-ContainerLog 'FATAL: no clang-cl on PATH'; exit 1 }
$env:CC = 'clang-cl'
$env:CXX = 'clang-cl'
Write-ContainerLog "C and C++ through $($clangCl.Source) (CC, CXX)"

Set-Location $Workspace
if (-not (Test-Path .\Cargo.toml)) { Write-Host "FATAL: workspace $Workspace has no Cargo.toml"; exit 1 }

$profiles = @(
    @{ Name = 'debug';   Args = @('build', '--workspace', '--locked') },
    @{ Name = 'profile'; Args = @('build', '--workspace', '--locked', '--profile', 'profile') },
    @{ Name = 'release'; Args = @('build', '--workspace', '--locked', '--release') }
)

foreach ($p in $profiles) {
    Write-Host "`n==> cargo $($p.Args -join ' ')"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & cargo $p.Args
    if ($LASTEXITCODE -ne 0) { Write-Host "FAILED: $($p.Name) build (exit $LASTEXITCODE)"; exit $LASTEXITCODE }
    Write-Host ("<== {0} OK in {1:mm\:ss}" -f $p.Name, $sw.Elapsed)
}

foreach ($p in $profiles) {
    $src = Join-Path 'C:\ct' $p.Name
    $dst = Join-Path $Workspace "target\container\$($p.Name)"
    New-Item -ItemType Directory -Force -Path $dst | Out-Null
    Get-ChildItem $src -File | Where-Object { $_.Extension -match '^\.(exe|dll|pdb|lib)$' } | Copy-Item -Destination $dst -Force
    Write-Host "`n$($p.Name) artifacts -> target\container\$($p.Name):"
    Get-ChildItem $dst -File | ForEach-Object { Write-Host ("  {0,14:n0}  {1}" -f $_.Length, $_.Name) }
}

Write-Host "`nALL BUILDS SUCCEEDED"
exit 0
