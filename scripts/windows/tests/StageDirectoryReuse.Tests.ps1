#requires -Version 7.0

# This checks arm64 stage setup; it does not run Cargo or load staged DLLs.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\WindowsStageDirectory.Fixture.psm1') -ErrorAction Stop

Describe 'Arm64 Cargo test stage directory reuse' {
    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsScripts.Shared'

    It 'resets the destination before copying executables' {
        Assert-StageResetBeforeCopy -ScriptPath (Join-Path $PSScriptRoot '..\Stage-CrossTests.ps1') `
            -ResetCommand Initialize-BuildDirectory -PathArgument '$Destination'
    }

    It 'removes stale files and keeps a writable stage directory' {
        Assert-StageDirectoryCleared -TestRoot $TestDrive -RunReset {
            param($path) Initialize-BuildDirectory -Path $path -Empty
        }
    }

    It 'stops when the destination cannot be created' {
        Assert-StageDirectoryAllocationFailure -TestRoot $TestDrive -RunReset {
            param($path) Initialize-BuildDirectory -Path $path -Empty
        }
    }
}
