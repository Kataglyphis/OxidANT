#requires -Version 7.0

# Guards against generated artifacts sneaking into this repo's git index.
#
# The checks themselves are ANTfrastructure's (WindowsRepoHygiene.Common:
# Get-TrackedIgnoredFile and Get-TrackedGeneratedArtifact, each with its own
# suite upstream). Only this repo's ROOT and its list of generated-output paths
# live here, because what counts as "generated" is a property of this build.
#
# Why a Rust workspace needs it. `git rm --cached` was run over this tree twice
# already - build logs under logs\windows\ on 2026-09-14, and the flatpak repo
# before that - and both times .gitignore had named the path for months. An
# ignore rule only stops a NEW file from being added; it does nothing once a
# path is in the index, so nothing here would have reported the drift. Two
# shapes of mistake, and the two `It` blocks below are one each:
#
#   tracked AND ignored   - added with `git add -f`, or added before the rule
#   tracked and NOT ignored - committed before anyone wrote the rule at all
#
# The second is the one that hides: it is invisible to the first check, which
# is exactly how CTest output stayed in a sibling repo's index for months.
#
# NOTE: written for Pester 3.4.0 (what the Windows lanes pin) - no BeforeAll
# outside Describe, and the dash-less assertion syntax.

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
        # Git pathspecs, and the list is deliberately explicit.
        #
        # THE TRAILING FORM MATTERS. `'**/__pycache__/'` matches NOTHING -
        # measured, not assumed: a wildcard pathspec ending in a slash does not
        # expand to the files beneath it, so the check silently grades zero
        # paths and reports clean. `'**/__pycache__/*'` matches. A leading
        # directory name with no wildcard (`target/`) is a prefix match and is
        # fine as it stands.
        $generated = @(
            'target/'                   # cargo build output
            'target-msix/'              # the MSIX staging target dir
            'dist/'                     # dist\msix and dist\msi packaging output
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
