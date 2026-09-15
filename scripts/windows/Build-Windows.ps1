<#
.SYNOPSIS
  Windows build and packaging script for Rust projects.
  Similar pattern to BeschleunigerBallett's Build-Windows.ps1

.DESCRIPTION
  - Uses ANTfrastructure's WindowsBuild.Common.psm1 for structured logging,
    WindowsConfig.Common for config access, WindowsMsix.Common for the version
    parse and the whole MSIX pack (Get-PackageVersion, Invoke-MsixPackage),
    and WindowsScripts.Shared for guards.
  - Runs cargo build, test and lint by calling cargo DIRECTLY. It does NOT go
    through ANTfrastructure's windows/scripts/rust/Build-Windows.ps1: that script
    has no consumer, does `rustup component add` against this image's offline
    rustup, and builds --all-features, which needs vendor SDKs the image has
    not got. The line that used to claim otherwise was wrong from the day it
    was written.
  - Packages MSIX using local config and template, then MSI with WiX v4.
#>

param(
#requires -Version 7.0

  # NOTE: there is deliberately no -Configurations here. One used to be
  # declared and nothing ever read it, so `-Configurations gui_windows` was
  # accepted and silently ignored. The feature-matrix concept lives in
  # Invoke-WindowsConfigMatrix.ps1, which implements it properly and drives
  # Invoke-AppProfiles.ps1 per configuration. Use that script instead of
  # reintroducing the parameter here.
  [switch]$SkipMsix,
  [switch]$SkipMsi,
  [switch]$SkipBuild,
  [switch]$SkipTests,
  [switch]$Clean
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Get-OrDefault and Get-ConfigValue used to be defined here, byte-identical to
# ANTfrastructure's WindowsConfig.Common.psm1. They now come from that module -
# see the import block below.

# Assert-Command comes from ANTfrastructure's WindowsScripts.Shared.psm1. SDK
# tool lookup is no longer called from here at all: Invoke-MsixPackage resolves
# makeappx itself, and the Resolve-Executable that used to sit here recursed the
# whole Windows Kits tree, where the module consults VsDevCmd's
# WindowsSdkVerBinPath / WindowsSDKVersion first and scans newest-first.

# The version file is parsed by WindowsMsix.Common's Get-PackageVersion, once
# per packaging step, and no longer by a local Normalize-Version or by the two
# divergent inline parses that followed it. It handles the 'v' prefix, a
# missing component and the component count each packager needs (4 for an
# AppxManifest, 3 for an MSI ProductVersion).

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
# One bootstrap resolves every module ANTfrastructure-first, with
# scripts/windows/modules/ as the project-specific fallback. It replaces four
# near-identical hard-coded import blocks; a module that moves upstream is now
# picked up without editing this script, and a missing submodule reports the
# exact `git submodule update` command instead of a bare path.
#
# Import-BuildModule pulls WindowsScripts.Shared in whether or not it is listed,
# which is what the four blocks below had each worked around by hand: a nested
# Import-Module inside a .psm1 binds into THAT module's private scope and never
# reaches this session, so importing only WindowsBuild.Common left
# Resolve-WorkspacePath undefined.
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')

Import-BuildModule @(
  'WindowsScripts.Shared'   # Assert-Command, Resolve-WorkspacePath
  'WindowsBuild.Common'     # build context/log/step primitives, Sync-BuildArtifacts
  'WindowsConfig.Common'    # Get-OrDefault, Get-ConfigValue
  'WindowsMsix.Common'      # Get-PackageVersion, Invoke-MsixPackage
)

$defaultConfigPath = Join-Path $PSScriptRoot 'Build-Windows.config.psd1'
$configPath = Get-OrDefault $env:BUILD_WINDOWS_CONFIG $defaultConfigPath
if (-not (Test-Path $configPath)) {
  throw "Build config not found: $configPath"
}
$config = Import-PowerShellDataFile -Path $configPath

$workspaceRootEnvVar = Get-OrDefault $env:WORKSPACE_ROOT_ENV (Get-ConfigValue -Config $config -Path 'Build.WorkspaceRootEnv')
$workspaceEnvItem = Get-Item -Path "Env:$workspaceRootEnvVar" -ErrorAction SilentlyContinue
$workspaceRootFromEnv = if ($null -ne $workspaceEnvItem) { $workspaceEnvItem.Value } else { $null }
$workspaceRoot = Get-OrDefault $workspaceRootFromEnv $repoRoot
$workspacePath = Resolve-WorkspacePath -Path $workspaceRoot

$logDir = Get-OrDefault $env:BUILD_LOG_DIR (Get-ConfigValue -Config $config -Path 'Build.LogDir')

$cargoTargetDir = Get-OrDefault $env:CARGO_TARGET_DIR (Get-ConfigValue -Config $config -Path 'Build.CargoTargetDir')
$cargoFeatures = Get-OrDefault $env:CARGO_FEATURES ((Get-ConfigValue -Config $config -Path 'Build.CargoFeatures') -join ',')

$binary = Get-OrDefault $env:BINARY (Get-ConfigValue -Config $config -Path 'Msix.Binary')

$msixName = Get-OrDefault $env:MSIX_PACKAGE_NAME (Get-ConfigValue -Config $config -Path 'Msix.PackageName')
$msixPublisher = Get-OrDefault $env:MSIX_PUBLISHER (Get-ConfigValue -Config $config -Path 'Msix.Publisher')
$msixPublisherDisplayName = Get-OrDefault $env:MSIX_PUBLISHER_DISPLAY_NAME (Get-ConfigValue -Config $config -Path 'Msix.PublisherDisplayName')
$msixDisplayName = Get-OrDefault $env:MSIX_DISPLAY_NAME (Get-ConfigValue -Config $config -Path 'Msix.DisplayName')
$msixDescription = Get-OrDefault $env:MSIX_DESCRIPTION (Get-ConfigValue -Config $config -Path 'Msix.Description')
$msixVersion = Get-OrDefault $env:MSIX_VERSION (Get-ConfigValue -Config $config -Path 'Msix.Version')
$msixMinVersion = Get-OrDefault $env:MSIX_MIN_VERSION (Get-ConfigValue -Config $config -Path 'Msix.MinVersion')

$context = New-BuildContext -Workspace $workspacePath -LogDir $logDir -StopOnError

try {
  Open-BuildLog -Context $context

  Write-BuildLog -Context $context -Message "Workspace: $workspacePath"
  Write-BuildLog -Context $context -Message "Binary: $binary"
  Write-BuildLog -Context $context -Message "MSIX: $msixName"

  $fastBuildDir = Initialize-BuildCacheEnvironment -Context $context
  $isolatedWorkspace = Join-Path $fastBuildDir "workspace"

  Invoke-BuildStep -Context $context -StepName 'Sync Source' -Critical -Script {
    Sync-BuildArtifacts -Context $context -Source $workspacePath -Destination $isolatedWorkspace -ExcludeCommonRustAndCppCache
  } | Out-Null

  $originalWorkspacePath = $workspacePath
  $workspacePath = $isolatedWorkspace
  Set-Location -Path $workspacePath

  # The scoop-shims PATH prepend that used to sit here is gone. It pointed at
  # C:\Users\ContainerAdministrator\scoop\shims, which does not exist in the
  # family Windows image: ANTfrastructure's windows/Dockerfile.base installs the
  # toolchain proper and puts it on PATH itself, and scoop is a HOST-side
  # concern (windows/scripts/host/Install-ScoopTools.ps1). The block therefore
  # prepended a non-existent directory on every single run and hid the fact
  # that nothing here needs it.

  Invoke-BuildStep -Context $context -StepName 'Verify Toolchain' -Critical -Script {
    Assert-Command -Name 'cargo' -InstallHint 'Install Rust toolchain via rustup'
    Invoke-BuildExternal -Context $context -File 'rustup' -Parameters @('--version') | Out-Null
    Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('--version') | Out-Null
  } | Out-Null

  if ($Clean) {
    Invoke-BuildStep -Context $context -StepName 'Clean Build Artifacts' -Script {
      Write-BuildLog -Context $context -Message "Cleaning cargo build artifacts..."
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('clean') | Out-Null

      $flutterExe = Get-Command flutter -ErrorAction SilentlyContinue
      if ($flutterExe) {
        Write-BuildLog -Context $context -Message "Cleaning Flutter build artifacts..."
        Invoke-BuildExternal -Context $context -File 'flutter' -Parameters @('clean') | Out-Null
      } else {
        Write-BuildLogWarning -Context $context -Message "Flutter not found, skipping Flutter clean"
      }
    } | Out-Null
  }

  if (-not $SkipBuild) {
    Invoke-BuildStep -Context $context -StepName 'Security Checks (audit & deny)' -Script {
      # PINNED, and a failure to resolve the pin is fatal. `cargo install
      # --locked cargo-audit cargo-deny` with no --version resolves to
      # whatever crates.io serves that minute, so a new advisory-db schema or
      # a new default cargo-deny lint turns this step red with no commit
      # behind it and nothing to bisect. The try/catch that used to wrap the
      # install is gone with it: a swallowed install failure left the two
      # gates below running whatever happened to be on PATH, or nothing.
      #
      # The versions come from the image (baked in as environment variables),
      # else from the submodule's versions.env - the fleet's single owner of
      # both pins - parsed with ANTfrastructure's own ConvertFrom-VersionsEnv
      # rather than a fourth hand-rolled .env reader. Unresolvable throws.
      $versionsEnv = Join-Path $repoRoot 'third_party/ANTfrastructure/linux/scripts/01-core/versions.env'
      $pins = if (Test-Path $versionsEnv) { ConvertFrom-VersionsEnv -Path $versionsEnv } else { [ordered]@{} }

      function Resolve-CargoToolPin {
        param([Parameter(Mandatory)][string]$Name)
        $fromEnv = [Environment]::GetEnvironmentVariable($Name)
        if (-not [string]::IsNullOrWhiteSpace($fromEnv)) { return $fromEnv }
        if ($pins.Contains($Name) -and -not [string]::IsNullOrWhiteSpace($pins[$Name])) {
          return $pins[$Name]
        }
        throw ("$Name is not set and could not be read from $versionsEnv. It pins a " +
               'cargo tool whose verdict decides this step; installing it unpinned ' +
               'would let crates.io choose the version instead.')
      }

      $cargoAuditVersion = Resolve-CargoToolPin -Name 'CARGO_AUDIT_VERSION'
      $cargoDenyVersion = Resolve-CargoToolPin -Name 'CARGO_DENY_VERSION'
      Write-BuildLog -Context $context -Message "cargo-audit $cargoAuditVersion, cargo-deny $cargoDenyVersion"

      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('install', '--locked', '--version', $cargoAuditVersion, 'cargo-audit') | Out-Null
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('install', '--locked', '--version', $cargoDenyVersion, 'cargo-deny') | Out-Null

    # No try/catch around these two. Swallowing them into a warning is how
    # `licenses FAILED` shipped unnoticed: cargo-deny rejected xxhash-rust's
    # BSL-1.0 on every single build and the step still reported success. A
    # security gate that cannot fail is not a gate. Findings belong in
    # deny.toml (allow the licence, or ignore the advisory with a reason) -
    # not in a catch block here.
    Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('audit') | Out-Null
    Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('deny', 'check', 'advisories', 'licenses', 'bans', 'sources') | Out-Null
    } | Out-Null

    # NEVER `rustup component add` here. The image's rustup is offline - its
    # dist server is a file:// mirror that Install-RustToolchain.ps1 deletes
    # after installing - so the call can only ever fail, and the previous
    # skip-on-failure made both gates decorative: each finished in ~0.1s and
    # reported success, so neither had run even once (measured 2026-08-07).
    # Call the components directly and let a failure BE a failure. If they are
    # missing the image is wrong, not the code; ANTfrastructure now installs them
    # with `-c rustfmt -c clippy` and asserts them at image-build time.
    Invoke-BuildStep -Context $context -StepName 'Format Check' -Critical -Script {
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('fmt', '--all', '--', '--check') | Out-Null
    } | Out-Null

    # Default features on purpose. --all-features pulls onnxruntime_cuda and
    # onnxruntime_directml, which need vendor SDKs this image has not got; the
    # feature-matrix CI job lints the combinations that are actually buildable.
    Invoke-BuildStep -Context $context -StepName 'Linting (cargo clippy)' -Critical -Script {
      $clippyParams = @('clippy', '--all-targets')
      if (-not [string]::IsNullOrWhiteSpace($cargoFeatures)) {
        $clippyParams += @('--features', $cargoFeatures)
      }
      $clippyParams += @('--', '-D', 'warnings')
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters $clippyParams | Out-Null
    } | Out-Null

    if (-not $SkipTests) {
      Invoke-BuildStep -Context $context -StepName 'Unit Tests' -Critical -Script {
        $testParams = @('test', '--all', '--verbose')
        if (-not [string]::IsNullOrWhiteSpace($cargoFeatures)) {
          $testParams += @('--features', $cargoFeatures)
        }
        Invoke-BuildExternal -Context $context -File 'cargo' -Parameters $testParams | Out-Null
      } | Out-Null
    }

    Invoke-BuildStep -Context $context -StepName 'Release Build' -Critical -Script {
      $buildParams = @('build', '--release', '--package', 'kataglyphis_cli', '--bin', $binary)
      if (-not [string]::IsNullOrWhiteSpace($cargoFeatures)) {
        $buildParams += @('--features', $cargoFeatures)
      }
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters $buildParams | Out-Null
    } | Out-Null
  }

  # Invoke-BuildStep, NOT Invoke-BuildOptional. The latter is
  # `try { & $Script } catch { Write-BuildLogWarning }` and never registers the
  # step with the context, so packaging failures appeared in neither the
  # SUCCEEDED nor the FAILED list - a run with a broken MSI still printed
  # "7 steps, 7 succeeded, 0 failed (100% success rate)". If packaging was
  # asked for and it breaks, that is a failure and the summary must say so.
  if (-not $SkipMsix) {
    Invoke-BuildStep -Context $context -StepName 'MSIX Packaging' -Critical -Script {
      # Invoke-MsixPackage (WindowsMsix.Common) owns everything from "the
      # staging directory holds what goes in the package" onwards: makeappx
      # lookup, the four logo assets, the token expansion, the pack, and the
      # assertion that a file really appeared. Three consumers had each written
      # that orchestration out; this repo's copy was ~100 lines and is gone.
      #
      # STAGING STAYS HERE, which is the split the module documents: what goes
      # into the package is this project's business (a cargo release exe, the
      # DLLs beside it, resources/), and it is the only part the three
      # consumers did differently.
      #
      # CARGO_TARGET_DIR is a standard cargo variable and is commonly ABSOLUTE
      # (the in-container scripts here set C:\ct). PowerShell's Join-Path does
      # not collapse that the way Path.Combine would - `Join-Path 'C:\a' 'C:\b'`
      # yields 'C:\a\C:\b' - and MSIX packaging then died on
      # "The filename, directory name, or volume label syntax is incorrect".
      # Same IsPathRooted idiom this script already uses for the manifest
      # template path.
      $cargoTargetFullPath = if ([System.IO.Path]::IsPathRooted($cargoTargetDir)) {
        $cargoTargetDir
      } else {
        Join-Path $workspacePath $cargoTargetDir
      }
      $releaseDir = Join-Path $cargoTargetFullPath 'release'

      # Get-PackageVersion, not a local parse. This script used to read the
      # version file TWICE - here and in the MSI step below - with two
      # different fallbacks and two different component rules, which is the
      # exact divergence that function was written to end. 4 components here
      # because an AppxManifest rejects 3.
      $resolvedVersion = Get-PackageVersion -WorkspacePath $workspacePath -Default $msixVersion -Components 4

      $msixStaging = Join-Path $cargoTargetFullPath 'msix-staging'
      if (Test-Path $msixStaging) {
        Remove-Item $msixStaging -Recurse -Force
      }

      $exePath = Join-Path $releaseDir "$binary.exe"
      if (-not (Test-Path $exePath)) {
        throw "Expected executable not found: $exePath"
      }

      # -ExtraFiles, not -ResourcesDir: the module's -ResourcesDir flattens the
      # directory's CONTENTS into the package root, and this app looks for
      # resources\ beside the exe. Copying the directory itself keeps that.
      $extraFiles = @(
        Get-ChildItem -Path $releaseDir -Filter '*.dll' -File -ErrorAction SilentlyContinue |
          ForEach-Object { $_.FullName }
      )
      $resourcesSource = Join-Path $workspacePath 'resources'
      if (Test-Path $resourcesSource) { $extraFiles += $resourcesSource }

      $logoPath = Join-Path $workspacePath 'images\logo.png'
      if (-not (Test-Path $logoPath)) {
        $logoPath = Join-Path $workspacePath 'third_party\ANTfrastructure\images\logo.png'
      }
      if (-not (Test-Path $logoPath)) {
        Write-BuildLogWarning -Context $context -Message "Logo file not found, generating transparent placeholders"
        $logoPath = ''
      }

      $manifestTemplateRel = Get-ConfigValue -Config $config -Path 'Msix.ManifestTemplate'
      $manifestTemplatePath = if ([System.IO.Path]::IsPathRooted($manifestTemplateRel)) { $manifestTemplateRel } else { Join-Path $workspacePath $manifestTemplateRel }

      $distDir = Join-Path $workspacePath 'dist\msix'
      $packageFile = Join-Path $distDir "$msixName`_$resolvedVersion`_x64.msix"

      Write-BuildLog -Context $context -Message "Creating MSIX package: $packageFile"

      # ONE TokenMap, where this script used to expand the template twice. The
      # module escapes each value for XML and replaces ordinally, which is the
      # bug fix that matters: `-replace` treats its replacement as a
      # substitution TEMPLATE, so a display name or description containing
      # `$&` or `$1` was silently rewritten into the manifest.
      Invoke-MsixPackage -Context $context `
        -StagingDir $msixStaging `
        -ExePath $exePath `
        -ExtraFiles $extraFiles `
        -LogoPath $logoPath `
        -ManifestTemplatePath $manifestTemplatePath `
        -MakeAppxPath (Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msix.MakeAppxPath') '') `
        -OutputPath $packageFile `
        -TokenMap @{
          '__MSIX_NAME__'                   = $msixName
          '__MSIX_PUBLISHER__'              = $msixPublisher
          '__MSIX_VERSION__'                = $resolvedVersion
          '__MSIX_MIN_VERSION__'            = $msixMinVersion
          '__MSIX_DISPLAY_NAME__'           = $msixDisplayName
          '__MSIX_PUBLISHER_DISPLAY_NAME__' = $msixPublisherDisplayName
          '__MSIX_DESCRIPTION__'            = $msixDescription
          '__EXE_REL_PATH__'                = "$binary.exe"
          '__STORE_LOGO_REL__'              = 'Assets/StoreLogo.png'
          '__LOGO44_REL__'                  = 'Assets/Square44x44Logo.png'
          '__LOGO150_REL__'                 = 'Assets/Square150x150Logo.png'
        } | Out-Null

      Write-BuildLogSuccess -Context $context -Message "MSIX package created: $packageFile"
    }
  }

  # MSI packaging with WiX Toolset v4, driving wix.exe directly.
  #
  # NOT cargo-wix. 0.3.9 is its newest release and it shells out to WiX v3's
  # candle.exe + light.exe, neither of which exists here: ANTfrastructure installs
  # WiX 4.0.6 as a dotnet tool (a single wix.exe) in
  # windows/scripts/host/Install-ScoopTools.ps1 and points WIX=C:\WiX at it in
  # windows/Dockerfile.base. Every MSI run therefore died with
  # "The compiler application ('candle') does not exist at the 'C:\WiX' path",
  # which went unnoticed while this step still ran as optional. Calling wix.exe
  # keeps the image on one WiX generation instead of adding a second.
  $msiEnabled = Get-ConfigValue -Config $config -Path 'Msi.Enabled'
  if (-not $SkipMsi -and $msiEnabled) {
    Invoke-BuildStep -Context $context -StepName 'MSI Packaging' -Critical -Script {
      $wixExe = $null
      if (-not [string]::IsNullOrWhiteSpace($env:WIX)) {
        $candidate = Join-Path $env:WIX 'wix.exe'
        if (Test-Path $candidate) { $wixExe = $candidate }
      }
      if (-not $wixExe) {
        $wixExe = (Get-Command 'wix.exe' -ErrorAction SilentlyContinue).Source
      }
      if (-not $wixExe) {
        throw "WiX v4 (wix.exe) not found. Looked under `$env:WIX ('$env:WIX') and on PATH. The container image installs it via ANTfrastructure's windows/scripts/host/Install-ScoopTools.ps1."
      }
      Write-BuildLog -Context $context -Message "Using WiX: $wixExe"

      # The SAME parse the MSIX step above uses, from the same module, with the
      # component count as the only difference: MSI ProductVersion is
      # major.minor.build, an AppxManifest wants four. Until this call the file
      # was read twice in one script, with two fallbacks and two rules for a
      # 'v' prefix and a missing component - the divergence Get-PackageVersion
      # exists to end.
      $resolvedVersion = Get-PackageVersion -WorkspacePath $workspacePath -Default $msixVersion -Components 3

      $msiOutputName = Get-OrDefault $env:MSI_OUTPUT_NAME (Get-ConfigValue -Config $config -Path 'Msi.OutputName')
      if ([string]::IsNullOrWhiteSpace($msiOutputName)) {
        $msiOutputName = $binary
      }

      $msiDistDir = Join-Path $workspacePath 'dist\msi'
      New-Item -ItemType Directory -Path $msiDistDir -Force | Out-Null

      $msiFile = Join-Path $msiDistDir "$msiOutputName-$resolvedVersion-x64.msi"

      Write-BuildLog -Context $context -Message "Creating MSI package: $msiFile"

      # Msi.WxsFile has been in Build-Windows.config.psd1 all along and was
      # never read - the old cargo-wix call let it look for WXS files under
      # crates/cli/wix/, which does not exist, so it also failed with
      # "There are no WXS files to create an installer".
      $wxsRel = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.WxsFile') 'wix/main.wxs'
      $wxsPath = if ([System.IO.Path]::IsPathRooted($wxsRel)) { $wxsRel } else { Join-Path $workspacePath $wxsRel }
      if (-not (Test-Path $wxsPath)) {
        throw "WiX source not found: $wxsPath (Msi.WxsFile = '$wxsRel')."
      }

      # Same IsPathRooted guard as the MSIX step: CARGO_TARGET_DIR is usually
      # absolute in the container (C:\ct), and Join-Path would mangle it.
      $msiCargoTargetFullPath = if ([System.IO.Path]::IsPathRooted($cargoTargetDir)) {
        $cargoTargetDir
      } else {
        Join-Path $workspacePath $cargoTargetDir
      }
      $msiExePath = Join-Path (Join-Path $msiCargoTargetFullPath 'release') "$binary.exe"
      if (-not (Test-Path $msiExePath)) {
        throw "Expected executable not found: $msiExePath"
      }

      $licenseRel = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.LicenseFile') 'wix/License.rtf'
      $licenseRtf = if ([System.IO.Path]::IsPathRooted($licenseRel)) { $licenseRel } else { Join-Path $workspacePath $licenseRel }
      if (-not (Test-Path $licenseRtf)) {
        throw "License file not found: $licenseRtf (Msi.LicenseFile = '$licenseRel', referenced by $wxsPath)."
      }

      # Msi.ProductName and Msi.Manufacturer were declared in the config and
      # never read, while those same two strings sat hard-coded in the WXS --
      # two sources of truth, where editing the config silently did nothing.
      # They are preprocessor variables now, so the config is the only one.
      $msiProductName = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.ProductName') $msixDisplayName
      $msiManufacturer = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.Manufacturer') $msixPublisherDisplayName
      if ([string]::IsNullOrWhiteSpace($msiProductName)) {
        throw "Msi.ProductName is empty and Msix.DisplayName gave no fallback; $wxsPath requires it."
      }
      if ([string]::IsNullOrWhiteSpace($msiManufacturer)) {
        throw "Msi.Manufacturer is empty and Msix.PublisherDisplayName gave no fallback; $wxsPath requires it."
      }

      # The WXS takes every moving value as a preprocessor variable so it never
      # has to assume a target\release next to the workspace root, and never
      # duplicates a string the config already owns.
      $wixParams = @(
        'build',
        '-arch', 'x64',
        '-ext', 'WixToolset.UI.wixext',
        '-d', "Version=$resolvedVersion",
        '-d', "ExeSource=$msiExePath",
        '-d', "LicenseRtf=$licenseRtf",
        '-d', "ProductName=$msiProductName",
        '-d', "Manufacturer=$msiManufacturer",
        '-out', $msiFile,
        $wxsPath
      )
      Invoke-BuildExternal -Context $context -File $wixExe -Parameters $wixParams | Out-Null

      if (-not (Test-Path $msiFile)) {
        throw "wix.exe reported success but produced no file at $msiFile"
      }

      Write-BuildLogSuccess -Context $context -Message "MSI package created: $msiFile"
    }
  }

  Invoke-BuildStep -Context $context -StepName 'Sync Artifacts' -Critical -Script {
    $distSource = Join-Path $workspacePath 'dist'
    $distDest = Join-Path $originalWorkspacePath 'dist'
    if (Test-Path $distSource) {
      Write-BuildLog -Context $context -Message "Syncing distribution artifacts to $distDest"
      Sync-BuildArtifacts -Context $context -Source $distSource -Destination $distDest
    }
    $targetSource = Join-Path $workspacePath $cargoTargetDir
    $targetDest = Join-Path $originalWorkspacePath $cargoTargetDir
    if (Test-Path $targetSource) {
      Write-BuildLog -Context $context -Message "Syncing cargo target directory to $targetDest"
      Sync-BuildArtifacts -Context $context -Source $targetSource -Destination $targetDest -ExcludeCommonRustAndCppCache
    }
  } | Out-Null

  Write-BuildLogSuccess -Context $context -Message 'Windows build completed.'
} finally {
  Write-BuildSummary -Context $context
  Close-BuildLog -Context $context
}

if ($context.Results.Failed.Count -gt 0) {
  throw "Windows build completed with failures ($($context.Results.Failed.Count) steps failed)."
}

