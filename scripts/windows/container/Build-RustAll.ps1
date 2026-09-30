# In-container debug/profile/release build into local C:\ct, C:\ch to dodge wcifs renames; see third_party/ANTfrastructure/docs/windows-builds.md.
#requires -Version 7.0

# EAP stays 'Continue' and $LASTEXITCODE is checked by hand: native stderr handling varies by version.
$ProgressPreference = 'SilentlyContinue'

# The driver copies this hub module into scratch, since the staged sources exclude third_party.
Import-Module 'C:\host-scratch\WindowsContainerLog.Common.psm1' -Force

# Log to scratch too: the docker CLI pipe drops output intermittently on this host.
Start-ContainerLog -Path 'C:\host-scratch\in-container-build.log'

Write-ContainerLog "=== Rust container build: debug / profile / release ==="
Write-ContainerLog "cpus: $env:NUMBER_OF_PROCESSORS"
[void](Invoke-ContainerLoggedCommand 'rustc -vV')
if ((Invoke-ContainerLoggedCommand 'cargo --version') -ne 0) { Write-ContainerLog 'FATAL: cargo not usable'; exit 1 }

New-Item -ItemType Directory -Force -Path C:\ct, C:\ch | Out-Null
$env:CARGO_TARGET_DIR = 'C:\ct'
$env:CARGO_HOME = 'C:\ch'

Set-Location C:\ws-mnt
if (-not (Test-Path .\Cargo.toml)) { Write-Host 'FATAL: workspace mount C:\ws-mnt has no Cargo.toml'; exit 1 }

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

# cmd copy, not Move-Item: renames fail on this host's bind mounts, plain copies work.
foreach ($p in $profiles) {
    $src = Join-Path 'C:\ct' $p.Name
    $dst = "C:\ws-mnt\target\container\$($p.Name)"
    cmd /c "if not exist $dst mkdir $dst" | Out-Null
    Get-ChildItem $src -File | Where-Object { $_.Extension -match '^\.(exe|dll|pdb|lib)$' } | ForEach-Object {
        cmd /c "copy /y ""$($_.FullName)"" ""$dst"" >nul"
        if ($LASTEXITCODE -ne 0) { Write-Host "WARN: copy failed for $($_.Name)" }
    }
    Write-Host "`n$($p.Name) artifacts -> target\container\$($p.Name):"
    Get-ChildItem $dst -File | ForEach-Object { Write-Host ("  {0,14:n0}  {1}" -f $_.Length, $_.Name) }
}

Write-Host "`nALL BUILDS SUCCEEDED"
exit 0

