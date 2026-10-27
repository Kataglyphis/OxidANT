# Shared AST selection keeps package wiring tests on the same parsed source.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') `
  -Function ConvertTo-TestScriptAst, Get-TestAstCommands, Get-TestCommandArguments -ErrorAction Stop

function Get-PackageBinding {
  param($Ast, [string]$Name, [string]$Contains = '')
  $calls = @(Get-TestAstCommands -Ast $Ast -Name $Name | Where-Object { $_.Extent.Text.Contains($Contains) })
  if ($calls.Count -ne 1) { throw "Expected one $Name call containing '$Contains'" }
  return Get-TestCommandArguments -Command $calls[0]
}

function Assert-PackageGuard {
  param([string]$Source, [ValidateSet('msi', 'msix')][string]$Kind)
  $ast = ConvertTo-TestScriptAst -Text $Source
  if ($Kind -eq 'msix') {
    $assignments = @($ast.FindAll({ param($node)
      $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -eq '$logoPath'
    }, $true))
    if ($assignments.Count -ne 1 -or $assignments[0].Extent.Text -ne '$logoPath = Join-Path $workspacePath ''images\logo.png''') {
      throw 'MSIX must use the project logo without a fallback'
    }
    $bindings = Get-PackageBinding -Ast $ast -Name 'Invoke-MsixPackage'
    if ($bindings.LogoPath -ne '$logoPath' -or $bindings.ContainsKey('GenerateTransparentLogos')) {
      throw 'MSIX must pass the project logo without transparent placeholders'
    }
    return
  }
  $generator = Get-PackageBinding -Ast $ast -Name 'Invoke-BuildExternal' -Contains '$licenseGenerator'
  if ($generator.File -ne "'cmake'" -or -not $generator.Parameters.Contains('-DLICENSE_INPUT=$projectLicense') -or
      -not $generator.Parameters.Contains('-DLICENSE_OUTPUT=$licenseRtf')) { throw 'MSI must generate RTF from the project license' }
  $package = Get-PackageBinding -Ast $ast -Name 'Invoke-MsiPackage'
  if ($package.LicenseFile -ne '$licenseRtf') { throw 'MSI must install the generated RTF' }
}

Export-ModuleMember -Function Assert-PackageGuard
