#requires -Version 7.0
# Puts every file resources\models\models.lock names into -Destination, checked by SHA-256, through a cache: the hub's fetcher.
[CmdletBinding()]
param(
  # Default: the lock's own directory, where the app and the packaging steps look.
  [string]$Destination,
  [string]$Lock = (Join-Path $PSScriptRoot '..\..\resources\models\models.lock')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$fetcher = Join-Path $PSScriptRoot '..\..\third_party\ANTfrastructure\linux\scripts\lib\fetch-data-lock.py'
if (-not (Test-Path -LiteralPath $fetcher -PathType Leaf)) {
  throw "No hub fetcher at ${fetcher}: git submodule update --init third_party/ANTfrastructure"
}
$fetchArgs = @('-I', $fetcher, '--lock', $Lock)
if ($Destination) { $fetchArgs += @('--dest', $Destination) }
# Out-Host, so a caller that discards the pipeline still shows what was fetched.
& python @fetchArgs | Out-Host
if ($LASTEXITCODE -ne 0) { throw "fetch-data-lock.py exited $LASTEXITCODE for $Lock" }
