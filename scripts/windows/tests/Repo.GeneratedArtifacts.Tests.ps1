#requires -Version 7.0

# See AGENTS.md § Continuous integration; written for Pester 3.4.0 (no BeforeAll outside Describe, dash-less Should).

Describe 'Repo generated artifacts' {

    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsRepoHygiene.Common'

    $repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path

    It 'has no tracked file that is also gitignored' {
        $tracked = @(Get-TrackedIgnoredFile -RepoRoot $repoRoot)

        if ($tracked.Count -gt 0) {
            Write-Host 'Tracked files that .gitignore also excludes (generated artifacts committed by mistake):'
            $tracked | ForEach-Object { Write-Host "  $_" }
            Write-Host 'Fix with: git rm --cached <path> for each file listed above - do not relax .gitignore.'
            Write-Host 'If the file is tracked ON PURPOSE, the .gitignore rule is the thing that is wrong.'
        }

        $tracked.Count | Should Be 0
    }

    It 'has no tracked file under a known generated-output path' {
        # A wildcard pathspec ending in a slash matches nothing: write `'**/__pycache__/*'`, not `'**/__pycache__/'`.
        $generated = @(
            'target/'                   # cargo build output
            'target-msix/'              # the MSIX staging target dir
            'dist/'                     # dist\windows-<arch>: bundle, msix and msi packaging output
            'logs/'                     # Build-Windows.ps1 / container build logs
            'debug/'                    # container-built binaries copied to the repo root
            'profile/'                  # ... by scripts/windows/container/
            'release/'                  # ...
            'packaging/flatpak/repo/'   # flatpak-builder's OSTree repo
            '**/__pycache__/*'
            '*.profraw'                 # llvm coverage
        )
        $tracked = @(Get-TrackedGeneratedArtifact -RepoRoot $repoRoot -Pattern $generated)

        if ($tracked.Count -gt 0) {
            Write-Host 'Tracked files under a generated-output path:'
            $tracked | ForEach-Object { Write-Host "  $_" }
            Write-Host 'Fix with: git rm -r --cached <path>, then add the path to .gitignore.'
        }

        $tracked.Count | Should Be 0
    }
}
