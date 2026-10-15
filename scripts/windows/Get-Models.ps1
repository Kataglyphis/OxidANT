#requires -Version 7.0
# Puts every file resources\models\models.lock names into -Destination and checks its SHA-256; a mismatch throws.
[CmdletBinding()]
param(
  # Default: the lock's own directory, where the app and the packaging steps look.
  [string]$Destination,
  [string]$Lock = (Join-Path $PSScriptRoot '..\..\resources\models\models.lock')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not (Test-Path -LiteralPath $Lock -PathType Leaf)) { throw "No lock file $Lock" }
if (-not $Destination) { $Destination = Split-Path -Parent (Resolve-Path -LiteralPath $Lock).Path }
$null = New-Item -ItemType Directory -Force -Path $Destination

foreach ($line in Get-Content -LiteralPath $Lock) {
  if ($line -match '^\s*(#|$)') { continue }
  $sha, $file, $url = -split $line
  $target = Join-Path $Destination $file
  if ((Test-Path -LiteralPath $target) -and ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -eq $sha)) { continue }
  $part = "$target.part"
  Invoke-WebRequest -Uri $url -OutFile $part -MaximumRetryCount 3 -RetryIntervalSec 5
  $got = (Get-FileHash -LiteralPath $part -Algorithm SHA256).Hash
  if ($got -ne $sha) {
    Remove-Item -LiteralPath $part -Force
    throw "${url}: sha256 $got, the lock says $sha"
  }
  Move-Item -LiteralPath $part -Destination $target -Force
  Write-Host "fetched $target (sha256 $($sha.Substring(0, 12)))"
}
