#requires -Version 7.0

# This runs only the source-copy loop; it does not build Cargo tests or load staged DLLs.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') -Function Get-TestSourceCopyLoopText, Invoke-TestSourceCopyLoop -ErrorAction Stop

function Test-StageShaderCopy {
    param([Parameter(Mandatory)][string]$Loop, [Parameter(Mandatory)][string]$Root)
    $Destination = Invoke-TestSourceCopyLoop -Loop $Loop -Root $Root -SourceVariableName repoRoot -DestinationVariableName Destination -Files @{
        'crates\webgpu_renderer\src\fixture.txt' = 'source'
        'crates\webgpu_renderer\tests\assets\fixture.txt' = 'asset'
        'shaders\forward\fixture.txt' = 'shader'
    }
    return (Test-Path -LiteralPath (Join-Path $Destination 'shaders\forward\fixture.txt') -PathType Leaf)
}

Describe 'Arm64 Cargo test source closure' {
    $scriptPath = Join-Path $PSScriptRoot '..\Stage-CrossTests.ps1'
    $copyLoop = Get-TestSourceCopyLoopText -ScriptPath $scriptPath -LoopVariable rel

    It 'stages the owned forward shader beside the embedded CARGO_MANIFEST_DIR tree' {
        (Test-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..\shaders\forward\forward.slang') -PathType Leaf) | Should Be $true
        Test-StageShaderCopy -Loop $copyLoop -Root (Join-Path $TestDrive 'control') | Should Be $true
    }

    It 'rejects a copy-loop mutant that omits the forward shader' {
        $mutant = $copyLoop.Replace(", 'shaders\forward'", '')
        ($mutant -ne $copyLoop) | Should Be $true
        Test-StageShaderCopy -Loop $mutant -Root (Join-Path $TestDrive 'mutant') | Should Be $false
    }
}
