#requires -Version 7.0
# This checks the setup's wiring and the MSI identity it shares; it runs neither makensis nor a setup.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') `
  -Function ConvertTo-TestScriptAst, Get-TestAstCommands, Get-TestCommandArguments, Get-ScriptFunctionDefinition, Import-FunctionModule, Assert-Throws -ErrorAction Stop
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\modules\WindowsConfig.Common.psm1') -ErrorAction Stop

# The setup table as key -> value source text.
function Get-NsisSetupTable($Ast) {
  $assignment = @($Ast.FindAll({ param($node)
    $node -is [System.Management.Automation.Language.AssignmentStatementAst] -and $node.Left.Extent.Text -eq '$setup' }, $true))
  if ($assignment.Count -ne 1) { throw "Expected one `$setup assignment, found $($assignment.Count)" }
  $table = $assignment[0].Right.Find({ param($node) $node -is [System.Management.Automation.Language.HashtableAst] }, $true)
  $map = @{}
  foreach ($pair in $table.KeyValuePairs) { $map[$pair.Item1.Extent.Text] = $pair.Item2.Extent.Text }
  return $map
}

function Get-SetupGateCall($Ast, [string]$Name) {
  return @(Get-TestAstCommands -Ast $Ast -Name $Name | Where-Object { $_.Extent.Text.Contains('$setupFile') })
}

Describe 'NSIS setup wiring' {
  $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  $scriptPath = (Resolve-Path (Join-Path $PSScriptRoot '..\Build-Windows.ps1')).Path
  $ast = ConvertTo-TestScriptAst -Text (Get-Content -LiteralPath $scriptPath -Raw)
  $setup = Get-NsisSetupTable $ast

  It 'starts gui from the setup, the MSI and the MSIX shortcut alike' {
    $setup['ShortcutArguments'] | Should Be "'gui'"
    $wix = [xml](Get-Content -LiteralPath (Join-Path $repoRoot 'wix\main.wxs') -Raw)
    $wix.SelectSingleNode("//*[local-name()='Shortcut' and @Id='ApplicationStartMenuShortcut']").GetAttribute('Arguments') | Should Be 'gui'
    $manifest = [xml](Get-Content -LiteralPath (Join-Path $repoRoot 'packaging\msix\AppxManifest.template.xml') -Raw)
    $application = $manifest.SelectSingleNode("//*[local-name()='Application' and @Id='App']")
    $application.GetAttribute('Parameters', 'http://schemas.microsoft.com/appx/manifest/uap/windows10/10') | Should Be 'gui'
  }

  It 'stops on an installed MSI through wix/main.wxs''s own UpgradeCode' {
    $setup['MsiUpgradeCode'] | Should Be '$identity.UpgradeCode'
    # No Manufacturer: the MSIX publisher's display name stands in, for the MSI and the setup alike.
    $prelude = "param(`$wxs)`n`$config = @{ Msi = @{ WxsFile = `$wxs; ProductName = 'OxidANT'; OutputName = 'kataglyphis_cli' } }`n" +
      "`$workspacePath = '$repoRoot'; `$binary = 'kataglyphis_cli'; `$msixDisplayName = 'unused'; `$msixPublisherDisplayName = 'Kataglyphis'"
    $text = Get-Content -LiteralPath $scriptPath -Raw
    $null = Import-FunctionModule -Text $text -Dir $TestDrive -FunctionName 'Get-MsiIdentity' -Prelude $prelude -ArgumentList @('wix/main.wxs')
    $wixCode = ([xml](Get-Content -LiteralPath (Join-Path $repoRoot 'wix\main.wxs') -Raw)).SelectSingleNode("//*[local-name()='Package']").GetAttribute('UpgradeCode')
    $identity = Get-MutMsiIdentity
    $identity.UpgradeCode | Should Be "{$($wixCode.Trim('{}').ToUpperInvariant())}"
    $identity.Manufacturer | Should Be 'Kataglyphis'
    $identity.ProductName | Should Be 'OxidANT'

    $wxs = Join-Path $TestDrive 'no-code.wxs'
    Set-Content -LiteralPath $wxs -Value "<Wix xmlns='http://wixtoolset.org/schemas/v4/wxs'><Package Name='x' /></Wix>"
    $null = Import-FunctionModule -Text $text -Dir $TestDrive -FunctionName 'Get-MsiIdentity' -Prelude $prelude -ArgumentList @($wxs)
    Assert-Throws -Body { Get-MutMsiIdentity } -MessagePattern 'names no Package UpgradeCode'
  }

  It 'signs with the per-build certificate, its password only in the environment' {
    $setup['SignPfx'] | Should Be '$signing.Pfx'
    $build = @(Get-TestAstCommands -Ast $ast -Name 'Invoke-NsisPackage')
    $build.Count | Should Be 1
    (Get-TestCommandArguments -Command $build[0]).Setup | Should Be '$setup'
    $step = @($ast.FindAll({ param($node)
      $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Invoke-NsisPackagingStep' }, $true))[0]
    $try = @($step.FindAll({ param($node) $node -is [System.Management.Automation.Language.TryStatementAst] }, $true))
    $try.Count | Should Be 1
    $try[0].Finally.Extent.Text.Contains('$env:MSIX_PFX_PASSWORD = $savedPassword') | Should Be $true
    $try[0].Finally.Extent.Text.Contains('Remove-Item -LiteralPath $signing.Pfx') | Should Be $true
  }

  It 'grades the written setup with the licence, ONNX Runtime and MSVC runtime gates' {
    (Get-SetupGateCall $ast 'Invoke-BuildExternal').Count | Should Be 1
    (Get-SetupGateCall $ast 'Assert-PayloadOrtLicenses').Count | Should Be 1
    (Get-SetupGateCall $ast 'Assert-PackageMsvcRuntimeNotice').Count | Should Be 1
    $setup['PayloadDir'] | Should Be '$payloadDir'
    $setup['OutFile'].Contains('packages\') | Should Be $true
  }

  It 'ships no setup from a GPU candidate' {
    $candidate = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\Build-GpuCandidate.ps1') -Raw
    $candidate.Contains('-SkipMsix -SkipMsi -SkipNsis') | Should Be $true
  }
}
