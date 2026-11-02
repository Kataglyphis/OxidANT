#requires -Version 7.0
<#
.SYNOPSIS
  Names the files models.lock pins; -Suffix demands exactly one match.
#>
[CmdletBinding()]
param(
  [string]$Suffix = '',
  [string]$Lock = (Join-Path $PSScriptRoot '..\..\resources\models\models.lock')
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$names = @(Get-Content -LiteralPath $Lock | ForEach-Object {
    $fields = @(-split $_)
    if ($fields.Count -ge 3 -and -not $fields[0].StartsWith('#')) { $fields[1] }
  })
if (-not $Suffix) {
  if ($names.Count -eq 0) { throw "$Lock pins no file" }
  return $names
}
$matched = @($names | Where-Object { $_.EndsWith($Suffix, [StringComparison]::Ordinal) })
if ($matched.Count -ne 1) { throw "$Lock pins $($matched.Count) file(s) ending in $Suffix, not one" }
$matched[0]
