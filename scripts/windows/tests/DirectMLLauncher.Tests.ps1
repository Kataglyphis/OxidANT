#requires -Version 7.0

Describe 'DirectML portable launcher' {
  BeforeEach {
    $launcher = Join-Path $TestDrive 'oxidant-directml.cmd'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\..\..\packaging\windows\oxidant-directml.cmd') -Destination $launcher
    Copy-Item -LiteralPath $env:ComSpec -Destination (Join-Path $TestDrive 'kataglyphis_cli.exe')
    $script:previousDevice = $env:KATAGLYPHIS_ORT_DEVICE
  }
  AfterEach {
    if ($null -eq $script:previousDevice) { Remove-Item Env:\KATAGLYPHIS_ORT_DEVICE -ErrorAction SilentlyContinue }
    else { $env:KATAGLYPHIS_ORT_DEVICE = $script:previousDevice }
  }
  It 'defaults to DirectML and forwards arguments' {
    Remove-Item Env:\KATAGLYPHIS_ORT_DEVICE -ErrorAction SilentlyContinue
    $result = & $launcher /d /c set KATAGLYPHIS_ORT_DEVICE
    $LASTEXITCODE | Should Be 0
    ($result -join "`n") | Should Match 'KATAGLYPHIS_ORT_DEVICE=directml'
    $forwarded = & $launcher /d /c echo 'argument with spaces'
    ($forwarded -join "`n") | Should Match 'argument with spaces'
  }
  It 'honors a CPU override and propagates the exit status' {
    $env:KATAGLYPHIS_ORT_DEVICE = 'cpu'
    $result = & $launcher /d /c set KATAGLYPHIS_ORT_DEVICE
    ($result -join "`n") | Should Match 'KATAGLYPHIS_ORT_DEVICE=cpu'
    & $launcher /d /c exit 37
    $LASTEXITCODE | Should Be 37
  }
  It 'defaults the NVIDIA launcher to CUDA and honors a CPU override' {
    $cudaLauncher = Join-Path $TestDrive 'oxidant-cuda.cmd'
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot '..\..\..\packaging\windows\oxidant-cuda.cmd') -Destination $cudaLauncher
    Remove-Item Env:\KATAGLYPHIS_ORT_DEVICE -ErrorAction SilentlyContinue
    $result = & $cudaLauncher /d /c set KATAGLYPHIS_ORT_DEVICE
    ($result -join "`n") | Should Match 'KATAGLYPHIS_ORT_DEVICE=cuda'
    $env:KATAGLYPHIS_ORT_DEVICE = 'cpu'
    $result = & $cudaLauncher /d /c set KATAGLYPHIS_ORT_DEVICE
    ($result -join "`n") | Should Match 'KATAGLYPHIS_ORT_DEVICE=cpu'
  }
}
