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
    # The forward shader sits beside the embedded CARGO_MANIFEST_DIR tree; the renderer and the bridge read kataglyphis_assets' glTF fixtures.
    $owned = @(
        @{ Tree = 'shaders\forward'; Owned = 'forward.slang'; Root = 'shader' }
        @{ Tree = 'crates\assets\tests\assets'; Owned = 'cube.gltf'; Root = 'fixtures' }
    )

    It 'stages <Tree>, which holds <Owned>, and a copy-loop mutant that omits it does not' -TestCases $owned {
        param($Tree, $Owned, $Root)
        (Test-Path -LiteralPath (Join-Path $PSScriptRoot "..\..\..\$Tree\$Owned") -PathType Leaf) | Should Be $true
        Test-StageCopy -Loop $copyLoop -Root (Join-Path $TestDrive "$Root-control") -Staged "$Tree\fixture.txt" | Should Be $true
        $mutant = $copyLoop.Replace(", '$Tree'", '')
        ($mutant -ne $copyLoop) | Should Be $true
        Test-StageCopy -Loop $mutant -Root (Join-Path $TestDrive "$Root-mutant") -Staged "$Tree\fixture.txt" | Should Be $false
    }
}
