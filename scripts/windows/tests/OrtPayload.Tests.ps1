#requires -Version 7.0

# WindowsOrtPayload.Common: a release payload carries the image's chain-built ONNX Runtime and nothing
# else (owner rule 2026-09-23), proved by the hub's G6 census. The DLLs are byte fixtures; the chain is
# a TestDrive ONNX_ROOT, so G6's reference is that fixture, as the image's prefix is in a real run.
# NOTE: written for Pester 3.4.0 - no BeforeAll outside Describe, dash-less Should,
# and no `Should Throw`, which never passes under pwsh 7 (measured 2026-09-23).

Describe 'WindowsOrtPayload.Common' {

    . (Join-Path $PSScriptRoot '..\Resolve-BuildModule.ps1')
    Import-BuildModule 'WindowsOrtPayload.Common'
    Import-BuildModule 'WindowsOrtProvenance.Common'

    $chainSrc = 'C:\temp\onnx-src\onnxruntime\core\session\inference_session.cc'
    $foreignSrc = 'C:\__w\1\s\onnxruntime\core\session\inference_session.cc'

    function New-FakeDll([string] $Path, [string] $Text) {
        New-Item -ItemType Directory -Force -Path (Split-Path $Path -Parent) | Out-Null
        [System.IO.File]::WriteAllBytes($Path, [System.Text.Encoding]::Latin1.GetBytes("MZ`0$Text`0"))
    }

    function Get-ThrowText([scriptblock] $Block) {
        try { & $Block | Out-Null } catch { return "$($_.Exception.Message)" }
        return ''
    }

    # A chain ONNX_ROOT, and a release dir whose exe loads ORT (OrtGetApiBase) or not.
    function New-Case([string] $Name, [switch] $PlainExe) {
        $case = Join-Path $TestDrive $Name
        $bin = Join-Path $case 'onnx\bin'
        New-FakeDll (Join-Path $bin 'onnxruntime.dll') "$chainSrc OrtGetApiBase"
        New-FakeDll (Join-Path $bin 'onnxruntime_providers_shared.dll') 'provider bridge'
        New-FakeDll (Join-Path $bin 'DirectML.dll') 'directml'
        New-FakeDll (Join-Path $case 'release\app.exe') $(if ($PlainExe) { 'main' } else { 'OrtGetApiBase' })
        return [pscustomobject]@{ Root = (Join-Path $case 'onnx'); Release = (Join-Path $case 'release'); Exe = (Join-Path $case 'release\app.exe'); Payload = (Join-Path $case 'payload') }
    }

    function Invoke-WithOnnxRoot([string] $Root, [scriptblock] $Block) {
        $saved = $env:ONNX_ROOT
        $env:ONNX_ROOT = $Root
        try { & $Block } finally { $env:ONNX_ROOT = $saved }
    }

    It 'tells an ORT-loading exe from a plain one the way G6 does' {
        $c = New-Case 'consumer'
        $p = New-Case 'plain' -PlainExe
        Test-ExeLoadsOrt -Path $c.Exe | Should Be $true
        Test-ExeLoadsOrt -Path $p.Exe | Should Be $false
    }

    It 'stages the chain ORT over stale ones and ships a payload G6 passes' {
        $c = New-Case 'green'
        New-FakeDll (Join-Path $c.Release 'onnxruntime.dll') $foreignSrc
        New-FakeDll (Join-Path $c.Release 'onnxruntime-genai.dll') 'stale'
        Invoke-WithOnnxRoot $c.Root {
            Copy-ChainOrtBeside -OnnxRoot $c.Root -Destination $c.Release
            $payload = New-OrtProvenPayload -ExePath $c.Exe -Destination $c.Payload
            $payload.LoadsOrt | Should Be $true
            $names = [string[]]@($payload.OrtDlls | ForEach-Object { Split-Path $_ -Leaf })
            [Array]::Sort($names, [StringComparer]::Ordinal)
            ($names -join ',') | Should Be 'DirectML.dll,onnxruntime.dll,onnxruntime_providers_shared.dll'
            Test-Path (Join-Path $c.Release 'onnxruntime-genai.dll') | Should Be $false
        }
    }

    It 'refuses a foreign, a stale and a missing ORT (G6 verdicts)' {
        $c = New-Case 'red'
        Invoke-WithOnnxRoot $c.Root {
            Copy-ChainOrtBeside -OnnxRoot $c.Root -Destination $c.Release
            New-FakeDll (Join-Path $c.Release 'onnxruntime.dll') $foreignSrc
            Get-ThrowText { New-OrtProvenPayload -ExePath $c.Exe -Destination $c.Payload } | Should Match 'FOREIGN'
            New-FakeDll (Join-Path $c.Release 'onnxruntime.dll') "$chainSrc FileVersion 1.27.0"
            Get-ThrowText { New-OrtProvenPayload -ExePath $c.Exe -Destination $c.Payload } | Should Match 'STALE'
            Remove-Item -LiteralPath (Join-Path $c.Release 'onnxruntime.dll')
            $missing = Get-ThrowText { New-OrtProvenPayload -ExePath $c.Exe -Destination $c.Payload }
            $missing | Should Match 'MISSING'
            $missing | Should Match 'UNRESOLVED'
        }
    }

    It 'refuses what G6 does not grade: a stray ORT-family name and another DirectML.dll' {
        $c = New-Case 'family'
        Invoke-WithOnnxRoot $c.Root {
            Copy-ChainOrtBeside -OnnxRoot $c.Root -Destination $c.Release
            New-FakeDll (Join-Path $c.Release 'onnxruntime_extra.dll') 'extra'
            Get-ThrowText { New-OrtProvenPayload -ExePath $c.Exe -Destination $c.Payload } | Should Match 'STRAY .*onnxruntime_extra'
            Remove-Item -LiteralPath (Join-Path $c.Release 'onnxruntime_extra.dll')
            New-FakeDll (Join-Path $c.Release 'DirectML.dll') 'other directml'
            Get-ThrowText { New-OrtProvenPayload -ExePath $c.Exe -Destination $c.Payload } | Should Match 'CHANGED .*DirectML'
        }
    }

    It 'ships a plain exe without ORT, and refuses a renamed ORT beside it' {
        $p = New-Case 'plain2' -PlainExe
        New-FakeDll (Join-Path $p.Release 'onnxruntime.dll') $chainSrc
        Invoke-WithOnnxRoot $p.Root {
            $payload = New-OrtProvenPayload -ExePath $p.Exe -Destination $p.Payload
            $payload.LoadsOrt | Should Be $false
            @($payload.OrtDlls).Count | Should Be 0
            New-FakeDll (Join-Path $p.Release 'helper.dll') $chainSrc
            Get-ThrowText { New-OrtProvenPayload -ExePath $p.Exe -Destination $p.Payload } | Should Match 'UNEXPECTED .*helper.dll'
        }
    }

    It 'counts an ORT-consuming DLL beside a plain exe: refused without ORT, proved with the chain one (mutation)' {
        $p = New-Case 'dll-user' -PlainExe
        Test-PayloadLoadsOrt -ExePath $p.Exe | Should Be $false
        New-FakeDll (Join-Path $p.Release 'oxidant.dll') 'OrtGetApiBase'
        Invoke-WithOnnxRoot $p.Root {
            Test-PayloadLoadsOrt -ExePath $p.Exe | Should Be $true
            $missing = Get-ThrowText { New-OrtProvenPayload -ExePath $p.Exe -Destination $p.Payload }
            $missing | Should Match 'MISSING'
            $missing | Should Match 'UNRESOLVED .*oxidant.dll'
            Copy-ChainOrtBeside -OnnxRoot $p.Root -Destination $p.Release
            $payload = New-OrtProvenPayload -ExePath $p.Exe -Destination $p.Payload
            $payload.LoadsOrt | Should Be $true
            (@($payload.OrtDlls | ForEach-Object { Split-Path $_ -Leaf }) -contains 'onnxruntime.dll') | Should Be $true
        }
    }

    It 'refuses to stage without a chain ONNX_ROOT' {
        $c = New-Case 'noroot'
        Get-ThrowText { Copy-ChainOrtBeside -OnnxRoot '' -Destination $c.Release } | Should Match 'ONNX_ROOT is unset'
        Get-ThrowText { Copy-ChainOrtBeside -OnnxRoot (Join-Path $TestDrive 'nowhere') -Destination $c.Release } | Should Match 'No chain ONNX Runtime'
    }

    It 'names the hub commit when the pinned hub lacks G6' {
        (Get-OrtCensusRequirement -Cause 'not found') | Should Match 'ORT single-source commit of 2026-09-23'
    }
}
