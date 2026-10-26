#requires -Version 7.0

# This checks arm64 stage setup; it does not run Cargo or load staged DLLs.
Describe 'Arm64 Cargo test stage directory reuse' {
    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsScripts.Shared'

    It 'resets the destination before copying executables' {
        $tokens = $null
        $errors = $null
        $ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot '..\Stage-CrossTests.ps1'), [ref]$tokens, [ref]$errors)
        @($errors | Where-Object { $_ }).Count | Should Be 0
        $calls = @($ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.CommandAst] }, $true))
        $reset = @($calls | Where-Object { $_.GetCommandName() -eq 'Initialize-BuildDirectory' })
        $reset.Count | Should Be 1
        ($reset[0].Extent.Text -match '-Path \$Destination -Empty') | Should Be $true
        $firstCopy = @($calls | Where-Object { $_.GetCommandName() -eq 'Copy-Item' } | Sort-Object { $_.Extent.StartOffset })[0]
        ($reset[0].Extent.StartOffset -lt $firstCopy.Extent.StartOffset) | Should Be $true
    }

    It 'removes stale files and keeps a writable stage directory' {
        $root = Join-Path $TestDrive 'stage'
        $null = New-Item -ItemType Directory -Path $root
        [IO.File]::WriteAllText((Join-Path $root 'stale.txt'), 'old')
        Initialize-BuildDirectory -Path $root -Empty
        (Test-Path -LiteralPath $root -PathType Container) | Should Be $true
        (Test-Path -LiteralPath (Join-Path $root 'stale.txt')) | Should Be $false
    }

    It 'stops when the destination cannot be created' {
        $target = Join-Path $TestDrive 'invalid<stage>'
        $message = try { Initialize-BuildDirectory -Path $target -Empty; '' } catch { $_.Exception.Message }
        [string]::IsNullOrWhiteSpace($message) | Should Be $false
        (Test-Path -LiteralPath $target) | Should Be $false
    }
}
