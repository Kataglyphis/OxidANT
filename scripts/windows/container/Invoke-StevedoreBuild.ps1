#requires -Version 7.0

<#
.SYNOPSIS
    Builds (and optionally tests) this Rust workspace in the family Windows image via Stevedore's docker.exe.

.DESCRIPTION
    Builds debug, profile and release into target\container\<profile>, mirrored to the gitignored repo root.
    The container plumbing is the hub's Invoke-RepoContainerBuild.ps1 (tar-pipe into a reused container); this
    file keeps only the spec. Host quirks: AGENTS.md § Inside the Stevedore Windows container.

.PARAMETER Test
    Also run the full test suite at the debug profile.

.PARAMETER TestOnly
    Run only the test suite, skipping the build.

.PARAMETER FreshContainer
    Discard the reusable build container, and with it the cargo target and home it keeps, first.
#>
[CmdletBinding(SupportsShouldProcess)]
param(
    [string]$DockerExe = '',
    # Empty: Invoke-RepoContainerBuild.ps1 asks Get-CiImageReference -Windows.
    [string]$Image = '',
    [switch]$Test,
    [switch]$TestOnly,
    [int]$MemoryGb = 48,
    [string]$ContainerName = 'kata-rust-build',
    [switch]$FreshContainer
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$ProgressPreference = 'SilentlyContinue'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path

$runner = Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\build\Invoke-RepoContainerBuild.ps1'
if (-not (Test-Path -LiteralPath $runner)) { throw "Required script not found: $runner (run: git submodule update --init --recursive third_party/ANTfrastructure)" }

$inboundExclude = @('.git', 'third_party/ANTfrastructure/third_party/DocumANTation')
$profiles = @('debug', 'profile', 'release')

function Invoke-ContainerPhase([string]$Script, [string[]]$OutputDirs, [string[]]$VerifyDirs = @()) {
    # A scriptblock gets the runner's workspace path; its tokens must be space-free (docker exec -> cmd /S /C).
    $command = { param($ws) @('pwsh', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "$ws\scripts\windows\container\$Script", '-Workspace', $ws) }.GetNewClosure()
    & $runner -RepoRoot $repoRoot -ContainerName $ContainerName -BuildCommand $command -Image $Image -DockerExe $DockerExe `
        -Isolation 'process' -MemoryGb $MemoryGb `
        -ExcludeRootEntries @('.git', '.vs', 'target', 'dist', 'logs', 'debug', 'profile', 'release', 'out') -InboundExclude $inboundExclude `
        -KeepDirs @('logs') -OutputDirs $OutputDirs -VerifyDirs $VerifyDirs -FreshContainer:$FreshContainer -WhatIf:$WhatIfPreference
    # Only the first phase may discard the container: the test phase reuses what the build left.
    $script:FreshContainer = $false
}

if (-not $TestOnly) {
    $profileDirs = @($profiles | ForEach-Object { "target\container\$_" })
    # VerifyDirs: every exe built in the container must have reached the host, which a green build alone does not prove.
    Invoke-ContainerPhase -Script 'Build-RustAll.ps1' -OutputDirs ($profileDirs + 'logs') -VerifyDirs $profileDirs
    if (-not $WhatIfPreference) {
        foreach ($p in $profiles) {
            robocopy (Join-Path $repoRoot "target\container\$p") (Join-Path $repoRoot $p) /MIR /NFL /NDL /NJH /NJS | Out-Null
            if ($LASTEXITCODE -ge 8) { throw "robocopy target\container\$p -> $p failed ($LASTEXITCODE)" }
        }
        $global:LASTEXITCODE = 0
        Write-Host "Artifacts: $repoRoot\target\container\{debug,profile,release} (+ root mirrors)" -ForegroundColor Green
    }
}
if ($Test -or $TestOnly) {
    Invoke-ContainerPhase -Script 'Test-RustAll.ps1' -OutputDirs @('logs')
}
if (-not $WhatIfPreference) { Write-Host "`nDone. Logs: $repoRoot\logs\in-container-*.log" }
