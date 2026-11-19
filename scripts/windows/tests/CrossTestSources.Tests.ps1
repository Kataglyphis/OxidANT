#requires -Version 7.0

# This runs only the source-copy loop; it does not build Cargo tests or load staged DLLs.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') -Function Get-TestSourceCopyLoopText, Invoke-TestSourceCopyLoop -ErrorAction Stop

function Test-StageCopy {
    param([Parameter(Mandatory)][string]$Loop, [Parameter(Mandatory)][string]$Root, [Parameter(Mandatory)][string]$Staged)
    $Destination = Invoke-TestSourceCopyLoop -Loop $Loop -Root $Root -SourceVariableName repoRoot -DestinationVariableName Destination -Files @{
        'crates\webgpu_renderer\src\fixture.txt' = 'source'
        'crates\assets\tests\assets\fixture.txt' = 'asset'
        'shaders\forward\fixture.txt' = 'shader'
    }
    return (Test-Path -LiteralPath (Join-Path $Destination $Staged) -PathType Leaf)
}

Describe 'Arm64 Cargo test source closure' {
    $scriptPath = Join-Path $PSScriptRoot '..\Stage-CrossTests.ps1'
    $copyLoop = Get-TestSourceCopyLoopText -ScriptPath $scriptPath -LoopVariable rel

    It 'stages the owned forward shader beside the embedded CARGO_MANIFEST_DIR tree' {
        (Test-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..\shaders\forward\forward.slang') -PathType Leaf) | Should Be $true
        Test-StageCopy -Loop $copyLoop -Root (Join-Path $TestDrive 'control') -Staged 'shaders\forward\fixture.txt' | Should Be $true
    }

    It 'rejects a copy-loop mutant that omits the forward shader' {
        $mutant = $copyLoop.Replace(", 'shaders\forward'", '')
        ($mutant -ne $copyLoop) | Should Be $true
        Test-StageCopy -Loop $mutant -Root (Join-Path $TestDrive 'mutant') -Staged 'shaders\forward\fixture.txt' | Should Be $false
    }

    It 'stages the glTF fixtures kataglyphis_assets owns and the renderer and the bridge read' {
        (Test-Path -LiteralPath (Join-Path $PSScriptRoot '..\..\..\crates\assets\tests\assets\cube.gltf') -PathType Leaf) | Should Be $true
        Test-StageCopy -Loop $copyLoop -Root (Join-Path $TestDrive 'fixtures') -Staged 'crates\assets\tests\assets\fixture.txt' | Should Be $true
    }

    It 'rejects a copy-loop mutant that omits the fixtures' {
        $mutant = $copyLoop.Replace(", 'crates\assets\tests\assets'", '')
        ($mutant -ne $copyLoop) | Should Be $true
        Test-StageCopy -Loop $mutant -Root (Join-Path $TestDrive 'fixtures-mutant') -Staged 'crates\assets\tests\assets\fixture.txt' | Should Be $false
    }
}
