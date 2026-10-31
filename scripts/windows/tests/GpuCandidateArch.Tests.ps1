#requires -Version 7.0

Describe 'Windows GPU candidate architecture' {
  function Write-FixturePe([string]$Path, [uint16]$Machine) {
    $bytes = [byte[]]::new(512)
    [BitConverter]::GetBytes([uint32]128).CopyTo($bytes, 0x3c)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($bytes, 128)
    [BitConverter]::GetBytes($Machine).CopyTo($bytes, 132)
    [IO.File]::WriteAllBytes($Path, $bytes)
  }

  BeforeEach {
    $script:fixture = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
    $scripts = Join-Path $script:fixture 'scripts\windows'
    $modules = Join-Path $script:fixture 'third_party\ANTfrastructure\windows\scripts\modules'
    $script:source = Join-Path $script:fixture 'source'
    $script:destination = Join-Path $script:fixture 'candidate'
    $null = New-Item -ItemType Directory -Force -Path $scripts, $modules, $script:source
    Copy-Item (Join-Path $PSScriptRoot '..\New-GpuCandidate.ps1') $scripts
    Copy-Item (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1') $scripts
    $hub = if ($env:ANTFRASTRUCTURE_DIR) { $env:ANTFRASTRUCTURE_DIR } else { Join-Path $PSScriptRoot '..\..\..\third_party\ANTfrastructure' }
    Copy-Item (Join-Path $hub 'windows\scripts\modules\WindowsTargetArch.Common.psm1') $modules
    Set-Content -LiteralPath (Join-Path $modules 'WindowsOrtPayload.Common.psm1') -Value @'
function Test-PayloadLoadsOrt { param($ExePath, $IncludeDirectory) throw 'PE_ARCH_ACCEPTED' }
Export-ModuleMember -Function Test-PayloadLoadsOrt
'@
    foreach ($name in 'WindowsOrtProvenance.Common', 'WindowsOrtLicense.Common', 'WindowsScripts.Shared') {
      Set-Content -LiteralPath (Join-Path $modules "$name.psm1") -Value 'Export-ModuleMember -Function @()'
    }
    foreach ($name in 'kataglyphis_cli.exe', 'onnxruntime.dll', 'onnxruntime_providers_shared.dll', 'DirectML.dll') {
      Write-FixturePe (Join-Path $script:source $name) 0x8664
    }
    foreach ($name in 'LICENSE', 'resources\models\deim_dfine_n_coco.onnx', 'licenses\onnxruntime\LICENSE', 'licenses\onnxruntime\ThirdPartyNotices.txt') {
      $path = Join-Path $script:source $name
      $null = New-Item -ItemType Directory -Force -Path (Split-Path -Parent $path)
      Set-Content -LiteralPath $path -Value 'fixture'
    }
    $script:candidateScript = Join-Path $scripts 'New-GpuCandidate.ps1'
  }

  It 'accepts x64 files and reaches the provenance gate' {
    $message = try { & $script:candidateScript -SourceBundle $script:source -Destination $script:destination; '' } catch { $_.Exception.Message }
    $message | Should Match 'PE_ARCH_ACCEPTED'
    Test-Path -LiteralPath $script:destination | Should Be $false
  }

  It 'rejects an ARM64 executable before making a candidate' {
    Write-FixturePe (Join-Path $script:source 'kataglyphis_cli.exe') 0xaa64
    $message = try { & $script:candidateScript -SourceBundle $script:source -Destination $script:destination; '' } catch { $_.Exception.Message }
    $message | Should Match 'GPU candidates require Windows x64'
    Test-Path -LiteralPath $script:destination | Should Be $false
  }

  It 'rejects an ARM64 provider even when the executable is x64' {
    Write-FixturePe (Join-Path $script:source 'DirectML.dll') 0xaa64
    $message = try { & $script:candidateScript -SourceBundle $script:source -Destination $script:destination; '' } catch { $_.Exception.Message }
    $message | Should Match 'GPU candidates require Windows x64'
    Test-Path -LiteralPath $script:destination | Should Be $false
  }

  It 'rejects a malformed executable' {
    [IO.File]::WriteAllText((Join-Path $script:source 'kataglyphis_cli.exe'), 'not a PE')
    $message = try { & $script:candidateScript -SourceBundle $script:source -Destination $script:destination; '' } catch { $_.Exception.Message }
    $message | Should Match 'too small to be a PE file'
    Test-Path -LiteralPath $script:destination | Should Be $false
  }

  It 'accepts a CUDA source without a DirectML DLL' {
    Remove-Item -LiteralPath (Join-Path $script:source 'DirectML.dll')
    $message = try { & $script:candidateScript -SourceBundle $script:source -Destination $script:destination -Provider cuda; '' } catch { $_.Exception.Message }
    $message | Should Match 'PE_ARCH_ACCEPTED'
    Test-Path -LiteralPath $script:destination | Should Be $false
  }
}
