<#
.SYNOPSIS
  Builds, lints, tests and packages the app into dist\windows-<arch> (bundle, MSIX, MSI).
.DESCRIPTION
  Calls cargo directly, not the hub's rust Build-Windows.ps1 (offline rustup, no --all-features).
  -TargetArch arm64 cross-builds and leaves audit/deny, fmt and tests to the x64 lane.
#>

param(
#requires -Version 7.0

  # No -Configurations on purpose: the feature matrix is Invoke-WindowsConfigMatrix.ps1's.
  [switch]$SkipMsix,
  [switch]$SkipMsi,
  [switch]$SkipBuild,
  [switch]$SkipTests,
  [switch]$Clean,
  # amd64 (alias x64) or arm64; empty means the image's WINDOWS_TARGET_ARCH, else amd64.
  [string]$TargetArch = ''
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
# Resolves modules ANTfrastructure-first, with scripts/windows/modules/ as the local fallback.
. (Join-Path $PSScriptRoot 'Resolve-BuildModule.ps1')

Import-BuildModule @(
  'WindowsScripts.Shared'   # Assert-Command, Resolve-WorkspacePath
  'WindowsBuild.Common'     # build context/log/step primitives, Sync-BuildArtifacts
  'WindowsConfig.Common'    # Get-OrDefault, Get-ConfigValue
  'WindowsMsix.Common'      # Get-PackageVersion, Invoke-MsixPackage
  'WindowsMsix.Signing'     # Invoke-MsixSign, which Invoke-MsixPackage -Sign calls
  'WindowsTargetArch.Common' # the arch facts: accepted spellings, cross or not, the Rust triple
  'WindowsCargoTarget.Common' # project-local: where each arch's build lands, what packages call it
)
# Chain ONNX Runtime staging and the G6 payload proof; a hub pin before ad08bc30 lacks the module.
try { Import-BuildModule @('WindowsOrtPayload.Common') } catch {
  throw "This build needs ANTfrastructure's WindowsOrtPayload.Common (hub commit ad08bc30 of 2026-09-25, third_party/ANTfrastructure/docs/onnxruntime-single-source.md § The shared Windows glue); move third_party/ANTfrastructure to it or later. ($($_.Exception.Message))"
}
# Package arch and DLL closure; an older hub pin lacks Get-ProductDllSearchPath, named here.
try {
  Import-BuildModule @('WindowsCrossBundle.Common')
  $null = Get-Command -Name 'Get-ProductDllSearchPath' -ErrorAction Stop
} catch {
  throw "This build needs ANTfrastructure's WindowsCrossBundle.Common with Get-ProductDllSearchPath (hub commit of 2026-09-25, third_party/ANTfrastructure/docs/windows-cross-builds.md); move third_party/ANTfrastructure to it or later. ($($_.Exception.Message))"
}

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
$gstPluginDir = Get-OrDefault $env:GSTREAMER_PLUGIN_DIR (Get-ConfigValue -Config $config -Path 'Build.GStreamerPluginDir')
$gstPlugins = @(Get-ConfigValue -Config $config -Path 'Build.GStreamerPlugins')

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

  # CARGO_TARGET_DIR is often absolute, and Join-Path would glue it on as 'C:\a\C:\b'.
  $targetRoot = if ([System.IO.Path]::IsPathRooted($cargoTargetDir)) { $cargoTargetDir } else { Join-Path $workspacePath $cargoTargetDir }
  $layout = Get-CargoTargetLayout -Arch $TargetArch -TargetRoot $targetRoot -WorkspacePath $workspacePath
  Write-BuildLog -Context $context -Message "Target: $($layout.Arch)$(if ($layout.IsCross) { " (cross: $($layout.CargoArgs -join ' '))" }), release dir $($layout.ReleaseDir)"
  if ($layout.IsCross) {
    # pkg-config refuses cross builds by default; the arm64 bundle's PKG_CONFIG_PATH is the target's.
    $env:PKG_CONFIG_ALLOW_CROSS = '1'
  }

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
    # Cross builds leave target-independent gates to the x64 lane; clippy still runs for the target.
    if ($layout.IsCross) {
      Write-BuildLog -Context $context -Message "Cross build: security checks, format check and unit tests are the x64 lane's."
    }
    if (-not $layout.IsCross) {
      Invoke-BuildStep -Context $context -StepName 'Security Checks (audit & deny)' -Script {
        # Pinned from the image env or the hub's versions.env, else fatal: unpinned, crates.io picks the verdict.
        if (-not (Get-Command -Name 'Get-ANTfrastructurePin' -ErrorAction SilentlyContinue)) {
          throw 'Get-ANTfrastructurePin is missing: move third_party/ANTfrastructure to hub 61cb0e42 (2026-10-05) or later.'
        }
        $cargoAuditVersion = Get-ANTfrastructurePin -Name 'CARGO_AUDIT_VERSION'
        $cargoDenyVersion = Get-ANTfrastructurePin -Name 'CARGO_DENY_VERSION'
        Write-BuildLog -Context $context -Message "cargo-audit $cargoAuditVersion, cargo-deny $cargoDenyVersion"

        Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('install', '--locked', '--version', $cargoAuditVersion, 'cargo-audit') | Out-Null
        Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('install', '--locked', '--version', $cargoDenyVersion, 'cargo-deny') | Out-Null

      # No try/catch: a gate that cannot fail is not a gate; findings belong in deny.toml.
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('audit') | Out-Null
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('deny', 'check', 'advisories', 'licenses', 'bans', 'sources') | Out-Null
      } | Out-Null

      # Never `rustup component add`: the image's rustup is offline, and the image installs both.
      Invoke-BuildStep -Context $context -StepName 'Format Check' -Critical -Script {
        Invoke-BuildExternal -Context $context -File 'cargo' -Parameters @('fmt', '--all', '--', '--check') | Out-Null
      } | Out-Null
    }

    # The workspace and all targets as on Linux, with the release features: only this lane compiles gui_windows. Not --all-features (GTK4).
    Invoke-BuildStep -Context $context -StepName 'Linting (cargo clippy)' -Critical -Script {
      $clippyParams = @('clippy', '--workspace', '--all-targets', '--locked') + $layout.CargoArgs
      if (-not [string]::IsNullOrWhiteSpace($cargoFeatures)) {
        $clippyParams += @('--features', ((@($cargoFeatures -split ',') | ForEach-Object { "kataglyphis_cli/$($_.Trim())" }) -join ','))
      }
      $clippyParams += @('--', '-D', 'warnings')
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters $clippyParams | Out-Null
    } | Out-Null

    if (-not $SkipTests -and -not $layout.IsCross) {
      Invoke-BuildStep -Context $context -StepName 'Unit Tests' -Critical -Script {
        $testParams = @('test', '--all', '--verbose')
        if (-not [string]::IsNullOrWhiteSpace($cargoFeatures)) {
          $testParams += @('--features', $cargoFeatures)
        }
        Invoke-BuildExternal -Context $context -File 'cargo' -Parameters $testParams | Out-Null
      } | Out-Null
    }

    Invoke-BuildStep -Context $context -StepName 'Release Build' -Critical -Script {
      $buildParams = @('build', '--release', '--package', 'kataglyphis_cli', '--bin', $binary) + $layout.CargoArgs
      if (-not [string]::IsNullOrWhiteSpace($cargoFeatures)) {
        $buildParams += @('--features', $cargoFeatures)
      }
      Invoke-BuildExternal -Context $context -File 'cargo' -Parameters $buildParams | Out-Null
    } | Out-Null

    # The exe loads ONNX Runtime at run time, so only the chain-built copy goes beside it.
    Invoke-BuildStep -Context $context -StepName 'Stage Chain ONNX Runtime' -Critical -Script {
      $releaseDir = $layout.ReleaseDir
      $exePath = Join-Path $releaseDir "$binary.exe"
      if (-not (Test-PayloadLoadsOrt -ExePath $exePath)) {
        Get-OrtFamilyFile -Directory $releaseDir | Remove-Item -Force
        Write-BuildLog -Context $context -Message "Neither $binary.exe nor a DLL beside it loads ONNX Runtime; nothing staged."
        return
      }
      $null = Copy-ChainOrtBeside -OnnxRoot "$env:ONNX_ROOT" -Destination $releaseDir
      $payload = New-OrtProvenPayload -ExePath $exePath -Destination (Join-Path $layout.ArchTargetDir 'ort-payload\stage')
      Write-BuildLog -Context $context -Message "Chain ONNX Runtime staged from $env:ONNX_ROOT\bin and proved by G6: $(@($payload.OrtDlls | ForEach-Object { Split-Path $_ -Leaf }) -join ', ')"
    } | Out-Null

    # Plugins are loaded by name, in no import table, so the config's list is staged explicitly.
    Invoke-BuildStep -Context $context -StepName 'Stage GStreamer Plugins' -Critical -Script {
      $exePath = Join-Path $layout.ReleaseDir "$binary.exe"
      $pluginTarget = Join-Path $layout.ReleaseDir 'lib\gstreamer-1.0'
      if (Test-Path -LiteralPath $pluginTarget) { Remove-Item -LiteralPath $pluginTarget -Recurse -Force }
      if (@(Get-PeImportNames -Path $exePath) -notcontains 'gstreamer-1.0-0.dll') {
        Write-BuildLog -Context $context -Message "$binary.exe does not link GStreamer; no plugins staged."
        return
      }
      $missing = @($gstPlugins | Where-Object { -not (Test-Path -LiteralPath (Join-Path $gstPluginDir "$_.dll") -PathType Leaf) })
      if ($missing.Count -gt 0) {
        throw "GStreamer plugins missing from $gstPluginDir`: $($missing -join ', ') (Build.GStreamerPlugins in Build-Windows.config.psd1)."
      }
      New-Item -ItemType Directory -Force -Path $pluginTarget | Out-Null
      foreach ($name in $gstPlugins) { Copy-Item -LiteralPath (Join-Path $gstPluginDir "$name.dll") -Destination $pluginTarget }
      Write-BuildLog -Context $context -Message "GStreamer plugins staged in $pluginTarget from $gstPluginDir`: $($gstPlugins -join ', ')"
    } | Out-Null

    # A clean machine has no C:\runtime or VC++ redist, so the whole import closure ships.
    Invoke-BuildStep -Context $context -StepName 'Stage DLL Closure' -Critical -Script {
      $exePath = Join-Path $layout.ReleaseDir "$binary.exe"
      $seeds = @($exePath) + @(Get-ChildItem -LiteralPath $layout.ReleaseDir -Filter '*.dll' -File | ForEach-Object FullName) +
        @(Get-ChildItem -LiteralPath (Join-Path $layout.ReleaseDir 'lib\gstreamer-1.0') -Filter '*.dll' -File -ErrorAction SilentlyContinue | ForEach-Object FullName)
      $search = @(Get-ProductDllSearchPath -Arch $layout.Arch)
      $copied = @(Copy-PeImportClosure -Path $seeds -SearchDirectory $search -Destination $layout.ReleaseDir -Arch $layout.Arch)
      Write-BuildLog -Context $context -Message "DLL closure staged beside $binary.exe from $($search -join ', '): $(@($copied | ForEach-Object { Split-Path $_ -Leaf }) -join ', ')"
    } | Out-Null
  }

  # The uploaded product: exe, DLLs, lib\ plugins and resources\, which the app expects beside it.
  Invoke-BuildStep -Context $context -StepName 'Portable Bundle' -Critical -Script {
    $bundleDir = Join-Path $layout.DistDir 'bundle'
    $bundle = New-OrtProvenPayload -ExePath (Join-Path $layout.ReleaseDir "$binary.exe") -Destination $bundleDir -IncludeDirectory 'lib'
    $resourcesSource = Join-Path $workspacePath 'resources'
    if (Test-Path $resourcesSource) { Copy-Item -LiteralPath $resourcesSource -Destination $bundleDir -Recurse -Force }
    Write-BuildLogSuccess -Context $context -Message "Portable bundle: $bundleDir ($(@($bundle.Dlls).Count) DLLs beside $binary.exe)"
  } | Out-Null

  # Invoke-BuildStep, not Invoke-BuildOptional, which hides failures from the summary.
  if (-not $SkipMsix) {
    Invoke-BuildStep -Context $context -StepName 'MSIX Packaging' -Critical -Script {
      # Staging is this project's; Invoke-MsixPackage owns everything from the staged tree on.
      $releaseDir = $layout.ReleaseDir

      # 4 components: an AppxManifest rejects 3.
      $resolvedVersion = Get-PackageVersion -WorkspacePath $workspacePath -Default $msixVersion -Components 4

      $msixStaging = Join-Path $layout.ArchTargetDir 'msix-staging'
      if (Test-Path $msixStaging) {
        Remove-Item $msixStaging -Recurse -Force
      }

      $exePath = Join-Path $releaseDir "$binary.exe"
      if (-not (Test-Path $exePath)) {
        throw "Expected executable not found: $exePath"
      }
      # Proved again here, since -SkipBuild skips the staging step.
      $payload = New-OrtProvenPayload -ExePath $exePath -Destination (Join-Path $layout.ArchTargetDir 'ort-payload\msix') -IncludeDirectory 'lib'

      # -ExtraFiles, not -ResourcesDir, which flattens resources\ into the package root.
      $extraFiles = @($payload.Dlls) + @($payload.Included)
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

      $distDir = Join-Path $layout.DistDir 'msix'
      $packageFile = Join-Path $distDir "$msixName`_$resolvedVersion`_$($layout.PackageArch).msix"

      Write-BuildLog -Context $context -Message "Creating MSIX package: $packageFile"

      # The module XML-escapes and replaces ordinally, unlike `-replace`, which expands `$1`.
      Invoke-MsixPackage -Context $context `
        -StagingDir $msixStaging `
        -ExePath $payload.Exe `
        -ExtraFiles $extraFiles `
        -LogoPath $logoPath `
        -ManifestTemplatePath $manifestTemplatePath `
        -MakeAppxPath (Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msix.MakeAppxPath') '') `
        -OutputPath $packageFile `
        -TokenMap @{
          '__MSIX_NAME__'                   = $msixName
          '__MSIX_PUBLISHER__'              = $msixPublisher
          '__MSIX_VERSION__'                = $resolvedVersion
          '__MSIX_ARCH__'                   = $layout.PackageArch
          '__MSIX_MIN_VERSION__'            = $msixMinVersion
          '__MSIX_DISPLAY_NAME__'           = $msixDisplayName
          '__MSIX_PUBLISHER_DISPLAY_NAME__' = $msixPublisherDisplayName
          '__MSIX_DESCRIPTION__'            = $msixDescription
          '__EXE_REL_PATH__'                = "$binary.exe"
          '__STORE_LOGO_REL__'              = 'Assets/StoreLogo.png'
          '__LOGO44_REL__'                  = 'Assets/Square44x44Logo.png'
          '__LOGO150_REL__'                 = 'Assets/Square150x150Logo.png'
        } `
        -Sign -SigningRoot $workspacePath | Out-Null

      Write-BuildLogSuccess -Context $context -Message "MSIX package created: $packageFile"
    }
  }

  # wix.exe (WiX v4) directly, not cargo-wix, which needs WiX v3's candle/light.
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

      # MSI ProductVersion is major.minor.build.
      $resolvedVersion = Get-PackageVersion -WorkspacePath $workspacePath -Default $msixVersion -Components 3

      $msiOutputName = Get-OrDefault $env:MSI_OUTPUT_NAME (Get-ConfigValue -Config $config -Path 'Msi.OutputName')
      if ([string]::IsNullOrWhiteSpace($msiOutputName)) {
        $msiOutputName = $binary
      }

      $msiDistDir = Join-Path $layout.DistDir 'msi'
      New-Item -ItemType Directory -Path $msiDistDir -Force | Out-Null

      $msiFile = Join-Path $msiDistDir "$msiOutputName-$resolvedVersion-$($layout.PackageArch).msi"

      Write-BuildLog -Context $context -Message "Creating MSI package: $msiFile"

      $wxsRel = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.WxsFile') 'wix/main.wxs'
      $wxsPath = if ([System.IO.Path]::IsPathRooted($wxsRel)) { $wxsRel } else { Join-Path $workspacePath $wxsRel }
      if (-not (Test-Path $wxsPath)) {
        throw "WiX source not found: $wxsPath (Msi.WxsFile = '$wxsRel')."
      }

      $msiExePath = Join-Path $layout.ReleaseDir "$binary.exe"
      if (-not (Test-Path $msiExePath)) {
        throw "Expected executable not found: $msiExePath"
      }
      # The MSI installs the payload G6 just proved, exe included.
      $msiPayload = New-OrtProvenPayload -ExePath $msiExePath -Destination (Join-Path $layout.ArchTargetDir 'ort-payload\msi') -IncludeDirectory 'lib'

      $licenseRel = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.LicenseFile') 'wix/License.rtf'
      $licenseRtf = if ([System.IO.Path]::IsPathRooted($licenseRel)) { $licenseRel } else { Join-Path $workspacePath $licenseRel }
      if (-not (Test-Path $licenseRtf)) {
        throw "License file not found: $licenseRtf (Msi.LicenseFile = '$licenseRel', referenced by $wxsPath)."
      }

      # The config owns these strings; the WXS takes them as preprocessor variables.
      $msiProductName = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.ProductName') $msixDisplayName
      $msiManufacturer = Get-OrDefault (Get-ConfigValue -Config $config -Path 'Msi.Manufacturer') $msixPublisherDisplayName
      if ([string]::IsNullOrWhiteSpace($msiProductName)) {
        throw "Msi.ProductName is empty and Msix.DisplayName gave no fallback; $wxsPath requires it."
      }
      if ([string]::IsNullOrWhiteSpace($msiManufacturer)) {
        throw "Msi.Manufacturer is empty and Msix.PublisherDisplayName gave no fallback; $wxsPath requires it."
      }

      # Every moving value is a preprocessor variable, so the WXS assumes no paths.
      $wixParams = @(
        'build',
        '-arch', $layout.PackageArch,
        '-ext', 'WixToolset.UI.wixext',
        '-d', "Version=$resolvedVersion",
        '-d', "ExeSource=$($msiPayload.Exe)",
        '-d', "LicenseRtf=$licenseRtf",
        '-d', "ProductName=$msiProductName",
        '-d', "Manufacturer=$msiManufacturer",
        '-out', $msiFile,
        $wxsPath
      )

      # One generated component per payload file for main.wxs's PayloadFiles, laid out like the bundle.
      $payloadFiles = [System.Collections.Generic.List[object]]::new()
      foreach ($dll in @($msiPayload.Dlls)) { $payloadFiles.Add([pscustomobject]@{ Source = $dll; Subdirectory = '' }) }
      $trees = @($msiPayload.Included | ForEach-Object { [pscustomobject]@{ Base = $msiPayload.Directory; Dir = $_ } })
      $resourcesSource = Join-Path $workspacePath 'resources'
      if (Test-Path -LiteralPath $resourcesSource) { $trees += [pscustomobject]@{ Base = $workspacePath; Dir = $resourcesSource } }
      foreach ($tree in $trees) {
        foreach ($file in @(Get-ChildItem -LiteralPath $tree.Dir -File -Recurse)) {
          $payloadFiles.Add([pscustomobject]@{ Source = $file.FullName; Subdirectory = [System.IO.Path]::GetRelativePath($tree.Base, $file.DirectoryName) })
        }
      }
      if ($payloadFiles.Count -gt 0) {
        $components = for ($i = 0; $i -lt $payloadFiles.Count; $i++) {
          $src = [System.Security.SecurityElement]::Escape($payloadFiles[$i].Source)
          $sub = if ($payloadFiles[$i].Subdirectory) { " Subdirectory='$([System.Security.SecurityElement]::Escape($payloadFiles[$i].Subdirectory))'" } else { '' }
          "      <Component Id='payload$i' Bitness='always64'$sub><File Id='payloadFile$i' Source='$src' KeyPath='yes'/></Component>"
        }
        $fragmentPath = Join-Path $layout.ArchTargetDir 'msi-payload-files.wxs'
        @(
          "<Wix xmlns='http://wixtoolset.org/schemas/v4/wxs'><Fragment>"
          "    <ComponentGroup Id='PayloadFiles' Directory='APPLICATIONFOLDER'>"
          $components
          '    </ComponentGroup>'
          '</Fragment></Wix>'
        ) | Set-Content -LiteralPath $fragmentPath -Encoding utf8
        $wixParams += @('-d', 'PayloadFiles=1', $fragmentPath)
        Write-BuildLog -Context $context -Message "MSI payload: $($payloadFiles.Count) files with $binary.exe, $(@($payloadFiles | Where-Object Subdirectory).Count) of them in subdirectories"
      }
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

