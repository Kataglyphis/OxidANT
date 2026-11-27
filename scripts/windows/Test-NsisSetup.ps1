#requires -Version 7.0
<#
.SYNOPSIS
  Installs the x64 setup silently on the bare runner, again as the upgrade, runs the installed CLI, then uninstalls it.
.DESCRIPTION
  A per-machine setup needs an elevated, throwaway host: the hosted runner is one, the image's Server Core cannot load
  the GUI's DLLs. The hub's arm64 device job does the same for the arm64 setup (container-ci-windows.yml).
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$PackagesDir,
  [string]$ProductName = 'OxidANT',
  [string]$MainExe = 'kataglyphis_cli.exe',
  [string]$UninstallKey = 'Kataglyphis.OxidANT',
  # RUNNER_TEMP first: GetTempPath can return an 8.3 name, which the shortcut's long target would not equal.
  [string]$InstallDir = (Join-Path ($env:RUNNER_TEMP ?? [IO.Path]::GetTempPath()) 'oxidant-setup-test')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$setups = @(Get-ChildItem -LiteralPath $PackagesDir -Filter '*-setup.exe' -File)
if ($setups.Count -ne 1) { throw "expected one *-setup.exe in $PackagesDir, found $($setups.Count)" }
$setup = $setups[0].FullName
$shortcut = Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs\$ProductName.lnk"
$registry = "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$UninstallKey"
$exe = Join-Path $InstallDir $MainExe

function Invoke-Silent([string]$File, [string[]]$Arguments) {
  $process = Start-Process -FilePath $File -ArgumentList $Arguments -Wait -PassThru
  if ($process.ExitCode) { throw "$(Split-Path $File -Leaf) $($Arguments -join ' ') exited $($process.ExitCode)" }
}

# A test certificate's root is untrusted, so the status is not Valid; the signer is what must be there.
$signature = Get-AuthenticodeSignature -FilePath $setup
if (-not $signature.SignerCertificate) { throw "$setup carries no Authenticode signature" }
Write-Host "Signed by $($signature.SignerCertificate.Subject) ($($signature.Status))"

# The second run is the upgrade: it finds the first install under the uninstall key and replaces it in place.
foreach ($pass in 'install', 'upgrade') {
  # /D= comes last and unquoted, as NSIS reads it.
  Invoke-Silent $setup @('/S', "/D=$InstallDir")
  if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { throw "$pass laid down no $exe" }
  $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
  if (-not (Test-Path -LiteralPath $shortcut) -or $link.TargetPath -ne $exe -or $link.Arguments -ne 'gui') {
    throw "$pass left no all-users shortcut that starts '$exe gui' ($shortcut -> '$($link.TargetPath)' '$($link.Arguments)')"
  }
  $entry = Get-ItemProperty -LiteralPath $registry
  if ($entry.QuietUninstallString -notlike "*$InstallDir\Uninstall.exe*") { throw "$pass registered '$($entry.QuietUninstallString)' under $registry" }
  & $exe onnx-runtime
  if ($LASTEXITCODE) { throw "the installed $MainExe onnx-runtime exited $LASTEXITCODE after the $pass" }
  Write-Host "$pass`: $exe, its shortcut and $registry are in place"
}

# _?= keeps the uninstaller in place, so -Wait waits for it, and it is the one file that may remain.
Invoke-Silent (Join-Path $InstallDir 'Uninstall.exe') @('/S', "_?=$InstallDir")
$left = @(Get-ChildItem -LiteralPath $InstallDir -Recurse -File -Force -ErrorAction SilentlyContinue | Where-Object Name -ne 'Uninstall.exe')
if ($left.Count) { throw "the uninstaller left $($left.Count) file(s):`n$($left.FullName -join "`n")" }
if (Test-Path -LiteralPath $shortcut) { throw "the uninstaller left $shortcut" }
if (Test-Path -LiteralPath $registry) { throw "the uninstaller left $registry" }
Remove-Item -LiteralPath $InstallDir -Recurse -Force
Write-Host "Uninstalled: no file but Uninstall.exe, no shortcut and no uninstall key remained"
