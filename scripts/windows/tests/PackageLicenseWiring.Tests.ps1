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
        @(Get-TestAstCommands -Ast $ast -Name 'Assert-MsiPackageLicense')
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
        $msi = @($checks | Where-Object { $_.GetCommandName() -eq 'Assert-MsiPackageLicense' })
        $msi.Count | Should Be 1
        $bindings = Get-TestCommandArguments -Command $msi[0]
        $bindings.Path | Should Be '$msiFile'
        $bindings.FileName | Should Be "'LICENSE'"
        $bindings.LicensePath | Should Be '$projectLicense'
        $bindings.VerifierPath | Should Be '$licenseVerifier'
        $source.Contains('payloadFile0=') | Should Be $false
        foreach ($check in $checks) {
            $mutant = $source.Replace($check.Extent.Text, "Write-Output 'removed'")
            @(Get-PackageLicenseChecks $mutant).Count | Should Be 2
        }
    }
}
