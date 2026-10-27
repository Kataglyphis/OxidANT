#requires -Version 7.0
# Checks RTF generation and WiX wiring; it does not install a built MSI.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') `
  -Function Assert-Throws -ErrorAction Stop
Import-Module (Join-Path $PSScriptRoot 'PackageWiring.Fixture.psm1') -ErrorAction Stop

Describe 'MSI project license source' {
  $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
  $source = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\Build-Windows.ps1') -Raw
  $generator = Join-Path $repoRoot 'wix\GenerateLicenseRtf.cmake'

  It 'uses the project license and passes the generated file to WiX' {
    Assert-PackageGuard -Source $source -Kind msi
    $mutant = $source.Replace('-DLICENSE_INPUT=$projectLicense', '-DLICENSE_INPUT=$otherLicense')
    if ($mutant -eq $source) { throw 'input mutation did not apply' }
    Assert-Throws -Body { Assert-PackageGuard -Source $mutant -Kind msi } -MessagePattern 'project license'
    $mutant = $source.Replace('-LicenseFile $licenseRtf', '-LicenseFile $otherLicense')
    if ($mutant -eq $source) { throw 'output mutation did not apply' }
    Assert-Throws -Body { Assert-PackageGuard -Source $mutant -Kind msi } -MessagePattern 'generated RTF'
  }

  It 'keeps the existing installer flow and installs the generated RTF' {
    $document = [xml](Get-Content -LiteralPath (Join-Path $repoRoot 'wix\main.wxs') -Raw)
    $file = $document.SelectSingleNode("//*[local-name()='File' and @Id='LicenseFile']")
    $file.Source | Should Be '$(var.LicenseRtf)'
    $document.SelectNodes("//*[local-name()='Publish' and @Dialog='WelcomeDlg' and @Value='CustomizeDlg']").Count | Should Be 1
  }

  It 'converts the complete current LICENSE through the shared encoder with paths containing spaces' {
    $directory = Join-Path $TestDrive 'license paths with spaces'
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $inputPath = Join-Path $repoRoot 'LICENSE'
    $output = Join-Path $directory 'project license.rtf'
    & cmake "-DLICENSE_INPUT=$inputPath" "-DLICENSE_OUTPUT=$output" -P $generator
    $LASTEXITCODE | Should Be 0
    $rtf = [IO.File]::ReadAllText($output) -replace "`r`n?", "`n"
    $prefix = '{\rtf1\ansi\uc1\deff0{\fonttbl{\f0 Arial;}}\fs20 '
    $rtf.StartsWith($prefix) | Should Be $true
    $rtf.EndsWith('}') | Should Be $true
    $plain = $rtf.Substring($prefix.Length, $rtf.Length - $prefix.Length - 1).Replace("\par`n", "`n")
    $expected = [IO.File]::ReadAllText($inputPath) -replace "`r`n?", "`n"
    $plain | Should Be $expected
  }

  It 'rejects a missing source or destination argument' {
    & cmake "-DLICENSE_INPUT=$(Join-Path $repoRoot 'LICENSE')" -P $generator 2>&1 | Out-Null
    $LASTEXITCODE | Should Not Be 0
  }

  It 'uses the supplied license rather than a cached project copy' {
    $inputPath = Join-Path $TestDrive 'updated-license.txt'
    $output = Join-Path $TestDrive 'updated-license.rtf'
    [IO.File]::WriteAllText($inputPath, 'Updated license holder 2030')
    & cmake "-DLICENSE_INPUT=$inputPath" "-DLICENSE_OUTPUT=$output" -P $generator
    $LASTEXITCODE | Should Be 0
    [IO.File]::ReadAllText($output).Contains('Updated license holder 2030') | Should Be $true
  }
}
