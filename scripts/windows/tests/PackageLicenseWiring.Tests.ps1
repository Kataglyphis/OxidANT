#requires -Version 7.0
# This checks license wiring, not the contents of a built package.

Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') `
    -Function ConvertTo-TestScriptAst, Get-TestAstCommands, Get-TestCommandArguments -ErrorAction Stop

function Get-PackageLicenseDestinations([string]$Source) {
    $ast = ConvertTo-TestScriptAst -Text $Source
    foreach ($call in @(Get-TestAstCommands -Ast $ast -Name 'Copy-PackageLicense')) {
        $bindings = Get-TestCommandArguments -Command $call
        if ($bindings.Path -ne '$projectLicense') { throw 'Unexpected project license source' }
        $bindings.Destination
    }
}

function Get-PackageLicenseChecks([string]$Source) {
    $ast = ConvertTo-TestScriptAst -Text $Source
    @(Get-TestAstCommands -Ast $ast -Name 'Invoke-BuildExternal' | Where-Object { $_.Extent.Text.Contains('$licenseVerifier') }) +
        @(Get-TestAstCommands -Ast $ast -Name 'Assert-MsiLicenseTexts')
}

function Get-ThirdPartyLicenseCalls([string]$Source, [string]$Name) {
    @(Get-TestAstCommands -Ast (ConvertTo-TestScriptAst -Text $Source) -Name $Name)
}

Describe 'Project license in Windows package payloads' {
    $source = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\Build-Windows.ps1') -Raw
    $destinations = @('$bundleDir', '$payload.Directory', '$msiPayload.Directory')

    It 'stages a project license for the bundle, MSIX and MSI' {
        $actual = @(Get-PackageLicenseDestinations $source)
        $actual.Count | Should Be 3
        foreach ($destination in $destinations) { @($actual | Where-Object { $_ -eq $destination }).Count | Should Be 1 }
    }

    It 'detects removing any package license call' {
        foreach ($destination in $destinations) {
            $call = 'Copy-PackageLicense -Path $projectLicense -Destination ' + $destination
            $mutant = $source.Replace($call, "Write-Output 'removed'")
            $mutant | Should Not Be $source
            @(Get-PackageLicenseDestinations $mutant).Count | Should Be 2
        }
    }

    It 'checks the staged bundle and finished MSIX and MSI with the shared verifier' {
        $checks = @(Get-PackageLicenseChecks $source)
        $checks.Count | Should Be 3
        @($checks | Where-Object { $_.Extent.Text.Contains('$bundleDir') }).Count | Should Be 1
        @($checks | Where-Object { $_.Extent.Text.Contains('$packageFile') }).Count | Should Be 1
        $msi = @($checks | Where-Object { $_.GetCommandName() -eq 'Assert-MsiLicenseTexts' })
        $msi.Count | Should Be 1
        $bindings = Get-TestCommandArguments -Command $msi[0]
        $bindings.MsiPath | Should Be '$msiFile'
        $bindings.WixPath | Should Be '$msiFragment'
        $bindings.Texts | Should Be '$msiTexts'
        $bindings.VerifierPath | Should Be '$licenseVerifier'
        $source.Contains('@($payloadLicense, $projectLicense)') | Should Be $true
        $member = Get-TestCommandArguments -Command @(Get-ThirdPartyLicenseCalls $source 'Assert-PackageLicense')[0]
        $member.Path | Should Be '$MsiPath'
        $member.Pattern | Should Be '$member'
        $source.Contains('Get-WixSourceFileId -WixPath $WixPath -SourcePath $text[0]') | Should Be $true
        $source.Contains('payloadFile0=') | Should Be $false
        foreach ($check in $checks) {
            $mutant = $source.Replace($check.Extent.Text, "Write-Output 'removed'")
            @(Get-PackageLicenseChecks $mutant).Count | Should Be 2
        }
    }

    It 'adds third-party texts to every payload and reads them back from each format' {
        $added = @(Get-ThirdPartyLicenseCalls $source 'Add-ThirdPartyLicenses' | ForEach-Object { (Get-TestCommandArguments -Command $_).Directory })
        $added.Count | Should Be 3
        foreach ($destination in $destinations) { @($added | Where-Object { $_ -eq $destination }).Count | Should Be 1 }
        @(Get-ThirdPartyLicenseCalls $source 'Get-ThirdPartyLicenseRules').Count | Should Be 2
        @(Get-ThirdPartyLicenseCalls $source 'Assert-PayloadOrtLicenses').Count | Should Be 3
        $source.Contains('$extraFiles += $msixLicenses.Root') | Should Be $true
        $source.Contains('@(@($msiPayload.Included) + $msiLicenses.Root') | Should Be $true
        $source.Contains("'--policy', `$cargoLicensePolicy, '--output-dir', `$cargoLicenses") | Should Be $true
        foreach ($name in 'Add-ThirdPartyLicenses', 'Get-ThirdPartyLicenseRules', 'Assert-PayloadOrtLicenses') {
            $first = (Get-ThirdPartyLicenseCalls $source $name)[0].Extent.Text
            $mutant = $source.Replace($first, "Write-Output 'removed'")
            @(Get-ThirdPartyLicenseCalls $mutant $name).Count | Should Be (@(Get-ThirdPartyLicenseCalls $source $name).Count - 1)
        }
    }
}
