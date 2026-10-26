#requires -Version 7.0

# This runs only the source-copy loop; it does not build Cargo tests or load staged DLLs.
Import-Module (Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure\windows\scripts\tests\TestHarness.psm1') -Function Get-TestScriptAst, Write-TestFileTree -ErrorAction Stop

function Get-StageSourceCopyLoop {
    param([Parameter(Mandatory)][string]$ScriptPath)
    $ast = Get-TestScriptAst -ScriptPath $ScriptPath
    $loops = @($ast.FindAll({
        param($node)
        if ($node -isnot [Management.Automation.Language.ForEachStatementAst]) { return $false }
        if ($node.Variable.VariablePath.UserPath -ne 'rel') { return $false }
        @($node.Body.FindAll({ param($inner) $inner -is [Management.Automation.Language.CommandAst] -and $inner.GetCommandName() -eq 'Copy-Item' }, $true)).Count -gt 0
    }, $true))
    if ($loops.Count -ne 1) { throw "Expected one relative source-copy loop, found $($loops.Count)" }
    return $loops[0].Extent.Text
}

function Test-StageShaderCopy {
    param([Parameter(Mandatory)][string]$Loop, [Parameter(Mandatory)][string]$Root)
    $repoRoot = Join-Path $Root 'source'
    $Destination = Join-Path $Root 'stage'
    Write-TestFileTree -Root $repoRoot -Files @{
        'crates\webgpu_renderer\src\fixture.txt' = 'source'
        'crates\webgpu_renderer\tests\assets\fixture.txt' = 'asset'
        'shaders\forward\fixture.txt' = 'shader'
    }
    $null = New-Item -ItemType Directory -Path $Destination -Force
    & ([scriptblock]::Create($Loop))
    return (Test-Path -LiteralPath (Join-Path $Destination 'shaders\forward\fixture.txt') -PathType Leaf)
}

Describe 'Arm64 Cargo test source closure' {
    $scriptPath = Join-Path $PSScriptRoot '..\Stage-CrossTests.ps1'
    $copyLoop = Get-StageSourceCopyLoop -ScriptPath $scriptPath

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
