#requires -Version 7.0
# This checks project icon wiring and shared logo validation, not a built installer or EXE resource.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') `
  -Function Get-TestScriptCommands, Get-TestCommandArguments, Assert-Throws -ErrorAction Stop
Import-Module (Join-Path $PSScriptRoot 'PackageWiring.Fixture.psm1') -ErrorAction Stop
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\modules\WindowsIcon.Common.psm1') -ErrorAction Stop
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\modules\WindowsMsix.Common.psm1') -ErrorAction Stop

function Assert-WixProjectIcon([string]$Source) {
  $document = [xml]$Source
  $icon = $document.SelectSingleNode("//*[local-name()='Icon' and @Id='ProductICO']")
  $shortcut = $document.SelectSingleNode("//*[local-name()='Shortcut' and @Id='ApplicationStartMenuShortcut']")
  $arp = $document.SelectSingleNode("//*[local-name()='Property' and @Id='ARPPRODUCTICON']")
  if (-not $icon -or $icon.SourceFile -ne '$(var.IconSource)') { throw 'MSI must embed the generated project icon' }
  if (-not $shortcut -or $shortcut.Icon -ne 'ProductICO') { throw 'MSI shortcut must use the project icon' }
  if (-not $arp -or $arp.Value -ne 'ProductICO') { throw 'Add/Remove Programs must use the project icon' }
}

Describe 'Windows MSI project icon' {
  $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  $scriptPath = Join-Path $PSScriptRoot '..\Build-Windows.ps1'
  $wixSource = Get-Content -LiteralPath (Join-Path $repoRoot 'wix\main.wxs') -Raw

  It 'passes the generated icon to WiX' {
    $converter = @(Get-TestScriptCommands -ScriptPath $scriptPath -Name 'ConvertTo-PythonAppIcon')
    $converter.Count | Should Be 1
    $converter[0].Extent.Text.Contains("'images\logo.png'") | Should Be $true
    $msi = @(Get-TestScriptCommands -ScriptPath $scriptPath -Name 'Invoke-MsiPackage')
    $msi.Count | Should Be 1
    (Get-TestCommandArguments -Command $msi[0]).WixVariables.Contains('IconSource = $projectIcon') | Should Be $true
  }

  It 'uses the same icon for the shortcut and Add/Remove Programs' { Assert-WixProjectIcon $wixSource }

  It 'uses only the project image for the MSIX logo' {
    $source = Get-Content -LiteralPath $scriptPath -Raw
    Assert-PackageGuard -Source $source -Kind msix
    $assignment = '$logoPath = Join-Path $workspacePath ''images\logo.png'''
    $foreign = '$logoPath = Join-Path $workspacePath ''third_party\ANTfrastructure\images\logo.png'''
    Assert-Throws -Body { Assert-PackageGuard -Source $source.Replace($assignment, $foreign) -Kind msix } -MessagePattern 'project logo'
    Assert-Throws -Body { Assert-PackageGuard -Source $source.Replace($assignment, "$assignment`n$foreign") -Kind msix } -MessagePattern 'project logo'
    Assert-Throws -Body { Assert-PackageGuard -Source $source.Replace('-LogoPath $logoPath', '-GenerateTransparentLogos') -Kind msix } -MessagePattern 'project logo'
  }

  It 'rejects missing and fully transparent project logos through the shared stager' {
    $missing = Join-Path $TestDrive 'missing.png'
    $stage = Join-Path $TestDrive 'msix-stage'
    $msixModule = Get-Module WindowsMsix.Common
    Assert-Throws -Body { & $msixModule { param($dir, $path) Write-MsixStageAssets -StagingDir $dir -LogoPath $path } $stage $missing } -MessagePattern 'logo not found'
    $transparent = Join-Path $TestDrive 'transparent.png'
    New-TransparentPng -Path $transparent -Width 8 -Height 8
    Assert-Throws -Body { & $msixModule { param($dir, $path) Write-MsixStageAssets -StagingDir $dir -LogoPath $path } $stage $transparent } -MessagePattern 'visible PNG'
  }

  It 'rejects a missing shortcut icon' {
    $mutant = $wixSource.Replace("Icon='ProductICO'", '')
    $mutant | Should Not Be $wixSource
    Assert-Throws -Body { Assert-WixProjectIcon $mutant } -MessagePattern 'shortcut'
  }

  It 'wraps the project PNG into the ICO without changing the image bytes' {
    $png = [IO.File]::ReadAllBytes((Join-Path $repoRoot 'images\logo.png'))
    $icoPath = ConvertTo-PythonAppIcon -PngPath (Join-Path $repoRoot 'images\logo.png') -Destination (Join-Path $TestDrive 'project.ico')
    $ico = [IO.File]::ReadAllBytes($icoPath)
    $ico.Length | Should Be ($png.Length + 22)
    [Convert]::ToBase64String($ico[22..($ico.Length - 1)]) | Should Be ([Convert]::ToBase64String($png))
    [Convert]::ToBase64String([IO.File]::ReadAllBytes((Join-Path $repoRoot 'images\logo.ico'))) | Should Be ([Convert]::ToBase64String($ico))
  }
}
