#requires -Version 7.0
# Runs the payload license helpers on synthetic trees; it does not build a package or inventory native DLLs.
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$hubModules = Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\modules'
Import-Module (Join-Path $repoRoot 'third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') `
  -Function ConvertTo-TestScriptAst, Assert-Throws -ErrorAction Stop
Import-Module (Join-Path $hubModules 'WindowsScripts.Shared.psm1') -ErrorAction Stop
Import-Module (Join-Path $hubModules 'WindowsMsix.Common.psm1') -ErrorAction Stop
Import-Module (Join-Path $hubModules 'WindowsMsiLicense.Common.psm1') -ErrorAction Stop

$buildScript = ConvertTo-TestScriptAst -Text (Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\Build-Windows.ps1') -Raw)
foreach ($name in 'Add-ThirdPartyLicenses', 'Get-ThirdPartyLicenseRules', 'Assert-PayloadOrtLicenses') {
  $definition = @($buildScript.FindAll({ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true))
  if ($definition.Count -ne 1) { throw "Build-Windows.ps1 must define $name once" }
  . ([scriptblock]::Create($definition[0].Extent.Text))
}
function Get-OrtChainWheelStore { 'C:\no-wheels' }
function Assert-OrtWheelLicensesInPackage { param($Path, $LicenseDirectory, $Format, $WixPath) $script:ortAssert = "$Path|$LicenseDirectory|$Format" }
function Copy-OrtWheelLicenses { param($OrtDll, $WheelDir, $Destination) $script:ortCall = "$OrtDll|$WheelDir"; Join-Path $Destination 'licenses\onnxruntime' }

Describe 'Third-party licenses in Windows payloads' {
  $verifier = Join-Path $repoRoot 'third_party\ANTfrastructure\linux\scripts\verify_package_license.py'
  $projectLicense = Join-Path $repoRoot 'LICENSE'
  $cargo = Join-Path $TestDrive 'cargo-licenses'
  $cli = Join-Path $cargo 'kataglyphis_cli@3.0.0\workspace'
  $crate = Join-Path $cargo 'dpi@0.1.2\package'
  New-Item -ItemType Directory -Force -Path $cli, $crate | Out-Null
  Copy-Item -LiteralPath $projectLicense -Destination (Join-Path $cli 'LICENSE')
  [IO.File]::WriteAllText((Join-Path $crate 'LICENSE'), "Apache License`n")
  [IO.File]::WriteAllText((Join-Path $cargo 'manifest.json'), "{`"packages`": []}`n")
  $model = Join-Path $TestDrive 'model.LICENSE'
  [IO.File]::WriteAllText($model, "model terms`n")

  # The CI runner exits with the last native status, so an expected verifier failure must not leak.
  function Invoke-Verifier([string]$Path, [string[]]$Rules) {
    & python $verifier $Path @Rules 2>&1 | Out-Null
    $code = $LASTEXITCODE
    $global:LASTEXITCODE = 0
    return $code
  }

  function New-Payload([string]$Name) {
    $payload = Join-Path $TestDrive $Name
    New-Item -ItemType Directory -Force -Path (Join-Path $payload 'licenses\cargo\stale'), (Join-Path $payload 'resources\models') | Out-Null
    Copy-Item -LiteralPath $projectLicense -Destination $payload
    Copy-Item -LiteralPath $model -Destination (Join-Path $payload 'resources\models')
    return $payload
  }

  It 'replaces the payload crate tree and passes the shared verifier' {
    $script:ortCall = ''
    $payload = New-Payload 'plain'
    $licenses = Add-ThirdPartyLicenses -Directory $payload -CargoLicenses $cargo
    Test-Path -LiteralPath (Join-Path $payload 'licenses\cargo\stale') | Should Be $false
    $licenses.Ort | Should Be ''
    $script:ortCall | Should Be ''
    $script:ortAssert = ''
    Assert-PayloadOrtLicenses -Path $payload -Licenses $licenses
    $script:ortAssert | Should Be ''
    $rules = Get-ThirdPartyLicenseRules -Licenses $licenses -ProjectLicense $projectLicense -ModelLicense $model
    Invoke-Verifier $payload $rules | Should Be 0
  }

  It 'fails when the crate manifest, the CLI license or the model license is missing' {
    foreach ($relative in 'licenses\cargo\manifest.json', 'licenses\cargo\kataglyphis_cli@3.0.0\workspace\LICENSE', 'resources\models\model.LICENSE') {
      $payload = New-Payload ('missing-' + ($relative -replace '[\\@.]', '-'))
      $licenses = Add-ThirdPartyLicenses -Directory $payload -CargoLicenses $cargo
      Remove-Item -LiteralPath (Join-Path $payload $relative)
      $rules = Get-ThirdPartyLicenseRules -Licenses $licenses -ProjectLicense $projectLicense -ModelLicense $model
      Invoke-Verifier $payload $rules | Should Not Be 0
    }
  }

  It 'stages the matching wheel texts only when the payload ships the ORT DLL' {
    $payload = New-Payload 'with-ort'
    [IO.File]::WriteAllText((Join-Path $payload 'onnxruntime.dll'), 'MZ')
    $licenses = Add-ThirdPartyLicenses -Directory $payload -CargoLicenses $cargo
    $licenses.Ort | Should Be (Join-Path $payload 'licenses\onnxruntime')
    $script:ortCall | Should Be "$(Join-Path $payload 'onnxruntime.dll')|C:\no-wheels"
    Assert-PayloadOrtLicenses -Path $payload -Licenses $licenses
    $script:ortAssert | Should Be "$payload|$($licenses.Ort)|archive"
  }

  It 'resolves the root license in the MSI fragment although crate trees repeat its basename' {
    $payload = New-Payload 'msi'
    $licenses = Add-ThirdPartyLicenses -Directory $payload -CargoLicenses $cargo
    $files = @(Get-ChildItem -LiteralPath $payload -File -Recurse | ForEach-Object {
        [pscustomobject]@{ Source = $_.FullName; Subdirectory = [IO.Path]::GetRelativePath($payload, $_.DirectoryName) -replace '^\.$', '' } })
    $fragment = New-WixPayloadFragment -PayloadFiles $files -Path (Join-Path $TestDrive 'payload.wxs')
    $rows = @($files | ForEach-Object { [pscustomobject]@{ Id = 'x'; FileName = Split-Path $_.Source -Leaf } })
    Assert-Throws -Body { Resolve-MsiLicenseFileId -FileName 'LICENSE' -Rows $rows } -MessagePattern 'matched 3 rows'
    $root = Get-WixSourceFileId -WixPath $fragment -SourcePath (Join-Path $payload 'LICENSE')
    $manifest = Get-WixSourceFileId -WixPath $fragment -SourcePath (Join-Path $licenses.Cargo 'manifest.json')
    $root | Should Match '^payloadFile\d+$'
    $manifest | Should Match '^payloadFile\d+$'
    $root | Should Not Be $manifest
  }
}
