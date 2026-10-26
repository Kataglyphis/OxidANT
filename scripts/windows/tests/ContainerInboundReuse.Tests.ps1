#requires -Version 7.0
# This checks root selection wiring; it does not run a container build.

Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\WindowsContainerInbound.Fixture.psm1') -ErrorAction Stop

Describe 'Windows container inbound reuse' {
    It 'lets the shared runner omit only Rust and packaging output at the root' {
        Assert-ContainerRootSelection -ScriptPath (Join-Path $PSScriptRoot '..\container\Invoke-StevedoreBuild.ps1') `
            -ExcludeEntries @('.git', '.vs', 'target', 'dist', 'logs', 'debug', 'profile', 'release', 'out')
    }
}
