#requires -Version 7.0

<#
.SYNOPSIS
    Builds (and optionally tests) this Rust workspace in the family Windows image via Stevedore's docker.exe.

.DESCRIPTION
    Builds debug, profile and release into target\container\<profile>, mirrored to the gitignored repo root.
    Host quirks: AGENTS.md § Inside the Stevedore Windows container.

.PARAMETER Test
    Also run the full test suite at the debug profile.

.PARAMETER TestOnly
    Run only the test suite, skipping the build.
#>
param(
    [string]$Docker = '',
    # Empty: filled by Get-CiImageReference after the import below, which a param default would precede.
    [string]$Image = '',
    # Off the Dev Drive, since the in-container scripts write their logs here constantly.
    [string]$StagingDir = (Join-Path $env:LOCALAPPDATA 'Temp\kataglyphis-rust-container'),
    # Mount a copy under $StagingDir\ws instead of the repo, for a host whose bindFlt refuses the volume.
    [switch]$StageSources,
    [switch]$Test,
    [switch]$TestOnly,
    [int]$MemoryGb = 48,
    [string]$ContainerName = 'kata-rust-build'
)

# EAP stays 'Continue', as native stderr can terminate under 'Stop'; exit codes are checked explicitly.
$ProgressPreference = 'SilentlyContinue'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path

# Container plumbing comes from the hub's modules, never inline: AGENTS.md § Inside the Stevedore Windows container.
$antfrastructureModules = Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\modules'
$reuseModule = Join-Path $antfrastructureModules 'WindowsContainerBuild.Reuse.psm1'
if (-not (Test-Path $reuseModule)) {
    throw "Required module not found: $reuseModule (run: git submodule update --init --recursive)"
}
Import-Module $reuseModule -Force

# Get-CiImageReference lives in its own module, as Reuse.psm1 does not re-export it.
$imageModule = Join-Path $antfrastructureModules 'WindowsContainerImage.Common.psm1'
if (-not (Test-Path $imageModule)) {
    throw "Required module not found: $imageModule (run: git submodule update --init --recursive)"
}
Import-Module $imageModule -Force
if ([string]::IsNullOrWhiteSpace($Image)) { $Image = Get-CiImageReference -Windows }
Write-Host "Using image: $Image"

# nerdctl is deliberately not a candidate: it talks to containerd, not the Windows lane's engine.
$Docker = Resolve-DockerExe -Override $Docker
Write-Host "Using docker: $Docker"

# Workspace: mount the repo itself; reads cross a bind mount fine, so only -StageSources hosts need a copy.
$scratch = Join-Path $StagingDir 'scratch'
New-Item -ItemType Directory -Force -Path $scratch | Out-Null

if ($StageSources) {
    $ws = Join-Path $StagingDir 'ws'
    New-Item -ItemType Directory -Force -Path $ws | Out-Null
    Write-Host "Staging sources -> $ws (-StageSources)"
    robocopy $repoRoot $ws /MIR /XD target .git .vs out dist debug profile release /XF *.msix /NFL /NDL /NJH /NJS | Out-Null
    if ($LASTEXITCODE -ge 8) { throw "robocopy staging failed ($LASTEXITCODE)" }
} else {
    $ws = $repoRoot
    Write-Host "Mounting repository directly -> $ws"
}

Copy-Item (Join-Path $PSScriptRoot 'Build-RustAll.ps1'), (Join-Path $PSScriptRoot 'Test-RustAll.ps1') -Destination $scratch -Force

# Staged into the scratch mount so the in-container scripts work alike under -StageSources, which copies no submodule.
$containerLogModule = Join-Path $antfrastructureModules 'WindowsContainerLog.Common.psm1'
if (-not (Test-Path $containerLogModule)) {
    throw "Required module not found: $containerLogModule"
}
Copy-Item $containerLogModule -Destination $scratch -Force

function Invoke-ContainerScript {
    param([Parameter(Mandatory)][string]$Script, [Parameter(Mandatory)][string]$Label)
    # A name the wcifs teardown still holds would hand Wait-ContainerExit the old exit code, so go unique.
    $runName = $ContainerName
    if (-not (Remove-BuildContainerSafe -DockerExe $Docker -Name $runName)) {
        $runName = "$runName-$([Guid]::NewGuid().ToString('N').Substring(0, 6))"
        Write-Warning "[$Label] falling back to '$runName' so this run cannot inherit the held container's exit code."
    }
    # --isolation process for the full host CPU count (Hyper-V isolation exposes 2).
    $isolationArgs = Get-ContainerIsolationArgs -Isolation 'process' -MemoryGb $MemoryGb
    Write-Host "`n==> [$Label] docker run $($isolationArgs -join ' ') --memory ${MemoryGb}g $Image" -ForegroundColor Cyan
    $keep = $false
    try {
        & $Docker run --name $runName @isolationArgs --memory "${MemoryGb}g" `
            --mount "type=bind,source=$ws,target=C:\ws-mnt" `
            --mount "type=bind,source=$scratch,target=C:\host-scratch" `
            $Image pwsh -NoProfile -ExecutionPolicy Bypass -File "C:\host-scratch\$Script"
        $clientExit = $LASTEXITCODE
        # The client pipe can drop while the container runs, so trust Wait-ContainerExit; 60 min is >10x a build.
        $exitCode = Wait-ContainerExit -DockerExe $Docker -Name $runName -Label $Label -TimeoutMinutes 60
        if ($clientExit -ne $exitCode) {
            Write-Warning "[$Label] docker client exited $clientExit but the container's real exit code is $exitCode (dropped client pipe). Trusting the container."
        }
        if ($exitCode -ne 0) { throw "[$Label] container run failed (exit $exitCode) -- see $scratch logs" }
    } catch {
        # Kept: Wait-ContainerExit's failure messages point the operator at `docker logs`.
        $keep = $true
        Write-Warning "[$Label] keeping container '$runName' for inspection: docker logs $runName (remove: docker rm -f $runName)"
        throw
    } finally {
        if (-not $keep) { [void](Remove-BuildContainerSafe -DockerExe $Docker -Name $runName) }
    }
    Write-Host "[$Label] OK" -ForegroundColor Green
}

if (-not $TestOnly) {
    Invoke-ContainerScript -Script 'Build-RustAll.ps1' -Label 'build'
    # Only a staged copy needs this: robocopy refuses to /MIR a directory over itself.
    if ($StageSources) {
        robocopy (Join-Path $ws 'target\container') (Join-Path $repoRoot 'target\container') /MIR /NFL /NDL /NJH /NJS | Out-Null
        if ($LASTEXITCODE -ge 8) { throw 'artifact copy-back failed' }
    }
    if (-not (Test-Path (Join-Path $repoRoot 'target\container'))) {
        throw "the build reported success but produced no target\container - nothing was delivered through the mount"
    }
    foreach ($p in 'debug', 'profile', 'release') {
        robocopy (Join-Path $repoRoot "target\container\$p") (Join-Path $repoRoot $p) /MIR /NFL /NDL /NJH /NJS | Out-Null
    }
    Write-Host "Artifacts: $repoRoot\target\container\{debug,profile,release} (+ root mirrors)" -ForegroundColor Green
}
if ($Test -or $TestOnly) {
    Invoke-ContainerScript -Script 'Test-RustAll.ps1' -Label 'test'
}
Write-Host "`nDone. Logs: $scratch\in-container-*.log"

