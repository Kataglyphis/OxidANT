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

    function Assert-OnePerDestination([object[]]$Actual) {
        $Actual.Count | Should Be 3
        foreach ($destination in $destinations) { @($Actual | Where-Object { $_ -eq $destination }).Count | Should Be 1 }
    }

    function Assert-SourceHas([string]$Text) { $source.Contains($Text) | Should Be $true }

    function Remove-SourceText([string]$Text) { $source.Replace($Text, "Write-Output 'removed'") }

    It 'stages a project license for the bundle, MSIX and MSI' {
        Assert-OnePerDestination @(Get-PackageLicenseDestinations $source)
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
        Assert-SourceHas '@($payloadLicense, $projectLicense)'
        $member = Get-TestCommandArguments -Command @(Get-ThirdPartyLicenseCalls $source 'Assert-PackageLicense')[0]
        $member.Path | Should Be '$MsiPath'
        $member.Pattern | Should Be '$member'
        Assert-SourceHas 'Get-WixSourceFileId -WixPath $WixPath -SourcePath $text[0]'
        $source.Contains('payloadFile0=') | Should Be $false
        foreach ($check in $checks) {
            @(Get-PackageLicenseChecks (Remove-SourceText $check.Extent.Text)).Count | Should Be 2
        }
    }

    It 'adds third-party texts to every payload and reads them back from each format' {
        Assert-OnePerDestination @(Get-ThirdPartyLicenseCalls $source 'Add-ThirdPartyLicenses' | ForEach-Object { (Get-TestCommandArguments -Command $_).Directory })
        @(Get-ThirdPartyLicenseCalls $source 'Get-ThirdPartyLicenseRules').Count | Should Be 2
        @(Get-ThirdPartyLicenseCalls $source 'Assert-PayloadOrtLicenses').Count | Should Be 3
        Assert-SourceHas '$extraFiles += $msixLicenses.Root'
        Assert-SourceHas '@(@($msiPayload.Included) + $msiLicenses.Root'
        Assert-SourceHas "'--policy', `$cargoLicensePolicy, '--output-dir', `$cargoLicenses"
        foreach ($name in 'Add-ThirdPartyLicenses', 'Get-ThirdPartyLicenseRules', 'Assert-PayloadOrtLicenses') {
            $mutant = Remove-SourceText (Get-ThirdPartyLicenseCalls $source $name)[0].Extent.Text
            @(Get-ThirdPartyLicenseCalls $mutant $name).Count | Should Be (@(Get-ThirdPartyLicenseCalls $source $name).Count - 1)
        }
    }
}
