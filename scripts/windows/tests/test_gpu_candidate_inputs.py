"""Exercise Windows GPU image preflight with shared modules and synthetic wheels."""

from __future__ import annotations

import os
import shutil
import struct
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HUB = Path(
    os.environ.get("ANTFRASTRUCTURE_DIR", ROOT / "third_party" / "ANTfrastructure")
)


def pe(machine: int, marker: int = 0) -> bytes:
    data = bytearray(512)
    struct.pack_into("<I", data, 0x3C, 128)
    struct.pack_into("<IH", data, 128, 0x4550, machine)
    data[200] = marker
    return bytes(data)


class GpuPreflight(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="oxidant-directml-")
        self.root = Path(self.temp.name)
        scripts = self.root / "scripts" / "windows"
        scripts.mkdir(parents=True)
        for name in (
            "Resolve-BuildModule.ps1",
            "Test-GpuCandidateInputs.ps1",
            "New-GpuCandidate.ps1",
            "Test-GpuCandidateArchive.ps1",
        ):
            shutil.copy2(ROOT / "scripts" / "windows" / name, scripts)
        self.hub = self.root / "third_party" / "ANTfrastructure"
        shutil.copytree(
            HUB / "windows" / "scripts" / "modules",
            self.hub / "windows" / "scripts" / "modules",
        )
        binary = self.hub / "linux" / "scripts" / "verify_package_binary.py"
        binary.parent.mkdir(parents=True)
        shutil.copy2(HUB / "linux" / "scripts" / binary.name, binary)
        shutil.copy2(HUB / "linux/scripts/verify_package_license.py", binary.parent)
        self.bin = self.root / "onnx" / "bin"
        self.bin.mkdir(parents=True)
        self.lib = self.root / "onnx" / "lib"
        self.lib.mkdir()
        for number, name in enumerate(
            (
                "onnxruntime.dll",
                "onnxruntime_providers_shared.dll",
                "DirectML.dll",
                "onnxruntime_providers_cuda.dll",
            )
        ):
            (self.bin / name).write_bytes(pe(0x8664, number))
        self.wheels = self.root / "wheels"
        self.wheels.mkdir()
        wheel_path = self.wheels / "onnxruntime_gpu-1.31.0-cp315-cp315-win_amd64.whl"
        with zipfile.ZipFile(wheel_path, "w") as wheel:
            for name in (
                "onnxruntime.dll",
                "onnxruntime_providers_shared.dll",
                "onnxruntime_providers_cuda.dll",
                "DirectML.dll",
            ):
                wheel.writestr(
                    f"onnxruntime/capi/{name}", (self.bin / name).read_bytes()
                )
            for name in ("LICENSE", "ThirdPartyNotices.txt"):
                wheel.writestr(f"onnxruntime/{name}", "fixture license")

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_preflight(
        self, image: bool = True, provider: str = "directml"
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ | {
            "ONNX_ROOT": str(self.root / "onnx"),
            "PYTHON_WHEELS": str(self.wheels),
            "TEMP": str(self.root),
            "TMP": str(self.root),
        }
        command = [
            "pwsh",
            "-NoProfile",
            "-File",
            str(self.root / "scripts/windows/Test-GpuCandidateInputs.ps1"),
            "-Provider",
            provider,
        ]
        if image:
            command.append("-Image")
        return subprocess.run(
            command, env=env, text=True, capture_output=True, check=False
        )

    def test_matching_image_and_host_preflight(self) -> None:
        for provider in ("directml", "cuda"):
            for image in (False, True):
                result = self.run_preflight(image, provider)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(list(self.root.glob("oxidant-gpu-input-*")))

    def test_missing_verifier_fails_before_image_check(self) -> None:
        (self.hub / "linux/scripts/verify_package_binary.py").unlink()
        result = self.run_preflight(False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lacks the finished-package binary verifier", result.stderr)

    def test_older_wheel_api_is_rejected(self) -> None:
        module = self.hub / "windows/scripts/modules/WindowsOrtLicense.Common.psm1"
        module.write_text(
            "function Copy-OrtWheelLicenses { param($OrtDll, $WheelDir, $Destination) }\n"
            "Export-ModuleMember -Function Copy-OrtWheelLicenses\n",
            encoding="utf-8",
        )
        result = self.run_preflight(False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("lacks same-wheel provider verification", result.stderr)

    def test_wrong_provider_bytes_are_rejected(self) -> None:
        (self.bin / "onnxruntime_providers_shared.dll").write_bytes(pe(0x8664, 7))
        result = self.run_preflight()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "differs from the matching wheel", " ".join(result.stderr.split())
        )

    def test_cuda_provider_must_match_the_same_wheel(self) -> None:
        (self.bin / "onnxruntime_providers_cuda.dll").write_bytes(pe(0x8664, 9))
        result = self.run_preflight(provider="cuda")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "differs from the matching wheel", " ".join(result.stderr.split())
        )

    def test_directml_must_match_the_same_wheel(self) -> None:
        (self.bin / "DirectML.dll").write_bytes(pe(0x8664, 9))
        result = self.run_preflight(provider="directml")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("differs from the matching wheel", " ".join(result.stderr.split()))

    def test_missing_cuda_provider_is_rejected(self) -> None:
        (self.bin / "onnxruntime_providers_cuda.dll").unlink()
        result = self.run_preflight(provider="cuda")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("onnxruntime_providers_cuda.dll", result.stderr)

    def test_cuda_and_bridge_are_resolved_from_lib(self) -> None:
        for name in (
            "onnxruntime_providers_shared.dll",
            "onnxruntime_providers_cuda.dll",
        ):
            (self.bin / name).rename(self.lib / name)
        result = self.run_preflight(provider="cuda")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_bin_provider_takes_precedence_over_lib(self) -> None:
        (self.lib / "onnxruntime_providers_cuda.dll").write_bytes(pe(0x8664, 9))
        result = self.run_preflight(provider="cuda")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        (self.bin / "onnxruntime_providers_cuda.dll").write_bytes(pe(0x8664, 7))
        result = self.run_preflight(provider="cuda")
        self.assertNotEqual(result.returncode, 0)

    def test_arm64_provider_is_rejected(self) -> None:
        (self.bin / "DirectML.dll").write_bytes(pe(0xAA64))
        result = self.run_preflight()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("GPU candidates require Windows x64", result.stderr)

    def test_arm64_cuda_provider_is_rejected(self) -> None:
        (self.bin / "onnxruntime_providers_cuda.dll").write_bytes(pe(0xAA64))
        result = self.run_preflight(provider="cuda")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("GPU candidates require Windows x64", result.stderr)

    def make_cuda_candidate_fixture(self) -> tuple[Path, Path]:
        """Stub the import census while using the real wheel and PE checks."""
        module = self.hub / "windows/scripts/modules/WindowsOrtPayload.Common.psm1"
        module.write_text(
            "function Test-PayloadLoadsOrt { param($ExePath, $IncludeDirectory) return $true }\n"
            "function Assert-ChainOrtTree { param($Root) }\n"
            "function Get-ChainOrtRuntimeFile { param($OnnxRoot, $Name)\n"
            '  $bin = Join-Path $OnnxRoot "bin/$Name"; $lib = Join-Path $OnnxRoot "lib/$Name"\n'
            "  if (Test-Path -LiteralPath $bin -PathType Leaf) { return $bin }\n"
            "  if (Test-Path -LiteralPath $lib -PathType Leaf) { return $lib }\n"
            '  throw "Required ONNX Runtime DLL $Name is missing"\n}\n'
            "function Copy-ChainOrtBeside { param($OnnxRoot, $Destination, [string[]]$RequiredProvider)\n"
            "  foreach ($n in @('onnxruntime.dll', 'onnxruntime_providers_shared.dll') + $RequiredProvider) {\n"
            "    Copy-Item -LiteralPath (Get-ChainOrtRuntimeFile -OnnxRoot $OnnxRoot -Name $n) -Destination $Destination -Force\n"
            "  }\n}\n"
            "Export-ModuleMember -Function Test-PayloadLoadsOrt, Assert-ChainOrtTree, Copy-ChainOrtBeside, Get-ChainOrtRuntimeFile\n",
            encoding="utf-8",
        )
        source = self.root / "source"
        source.mkdir()
        (source / "kataglyphis_cli.exe").write_bytes(pe(0x8664, 4))
        for name in ("onnxruntime.dll", "onnxruntime_providers_shared.dll", "DirectML.dll"):
            shutil.copy2(self.bin / name, source / name)
        for name in (
            "LICENSE",
            "resources/models/deim_dfine_n_coco.onnx",
            "licenses/onnxruntime/LICENSE",
            "licenses/onnxruntime/ThirdPartyNotices.txt",
        ):
            path = source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("fixture license", encoding="utf-8")
        assets = self.root / "packaging/windows"
        assets.mkdir(parents=True)
        for name in ("oxidant-cuda.cmd", "CUDA-README.txt", "oxidant-directml.cmd", "DIRECTML-README.txt"):
            shutil.copy2(ROOT / "packaging/windows" / name, assets)
        return source, self.root / "candidate"

    def run_cuda_candidate(
        self, source: Path, destination: Path, provider: str = "cuda"
    ) -> subprocess.CompletedProcess[str]:
        env = os.environ | {
            "ONNX_ROOT": str(self.root / "onnx"),
            "PYTHON_WHEELS": str(self.wheels),
            "TEMP": str(self.root),
            "TMP": str(self.root),
        }
        return subprocess.run(
            [
                "pwsh",
                "-NoProfile",
                "-File",
                str(self.root / "scripts/windows/New-GpuCandidate.ps1"),
                "-SourceBundle",
                str(source),
                "-Destination",
                str(destination),
                "-Provider",
                provider,
            ],
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def test_cuda_candidate_stages_provider_without_mutating_source(self) -> None:
        source, destination = self.make_cuda_candidate_fixture()
        (self.bin / "onnxruntime_providers_cuda.dll").rename(
            self.lib / "onnxruntime_providers_cuda.dll"
        )
        result = self.run_cuda_candidate(source, destination)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            (destination / "onnxruntime_providers_cuda.dll").read_bytes(),
            (self.lib / "onnxruntime_providers_cuda.dll").read_bytes(),
        )
        self.assertFalse((source / "onnxruntime_providers_cuda.dll").exists())
        self.assertTrue((destination / "oxidant-cuda.cmd").is_file())

    def test_cuda_candidate_refuses_bad_provider_before_copy(self) -> None:
        source, destination = self.make_cuda_candidate_fixture()
        (self.bin / "onnxruntime_providers_cuda.dll").write_bytes(pe(0x8664, 9))
        result = self.run_cuda_candidate(source, destination)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(destination.exists())

    def test_directml_candidate_rejects_changed_bundle_dll(self) -> None:
        source, destination = self.make_cuda_candidate_fixture()
        (source / "DirectML.dll").write_bytes(pe(0x8664, 9))
        result = self.run_cuda_candidate(source, destination, "directml")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("differs from the matching wheel", " ".join(result.stderr.split()))

    def make_archive_fixture(self) -> tuple[Path, Path]:
        bundle = self.root / "bundle"
        bundle.mkdir()
        files = {
            "kataglyphis_cli.exe": b"application",
            "LICENSE": b"project license",
            "resources/models/deim_dfine_n_coco.onnx": b"model",
            "oxidant-cuda.cmd": b"launcher",
            "CUDA-README.txt": b"readme",
            "onnxruntime.dll": b"core",
            "onnxruntime_providers_shared.dll": b"shared",
            "onnxruntime_providers_cuda.dll": b"cuda",
            "licenses/onnxruntime/LICENSE": b"ort license",
            "licenses/onnxruntime/ThirdPartyNotices.txt": b"ort notice",
        }
        for name, content in files.items():
            path = bundle / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)
        archive = self.root / "candidate.zip"
        with zipfile.ZipFile(archive, "w") as output:
            for name, content in files.items():
                output.writestr(name, content)
        return bundle, archive

    def run_archive_check(
        self, bundle: Path, archive: Path
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "pwsh",
                "-NoProfile",
                "-File",
                str(self.root / "scripts/windows/Test-GpuCandidateArchive.ps1"),
                "-Bundle",
                str(bundle),
                "-Archive",
                str(archive),
                "-Provider",
                "cuda",
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_finished_cuda_archive_matches_proven_bundle(self) -> None:
        bundle, archive = self.make_archive_fixture()
        result = self.run_archive_check(bundle, archive)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_extra_provider_in_finished_archive_is_rejected(self) -> None:
        bundle, archive = self.make_archive_fixture()
        with zipfile.ZipFile(archive, "a") as output:
            output.writestr("onnxruntime_providers_other.dll", b"extra provider")
        result = self.run_archive_check(bundle, archive)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unexpected ONNX Runtime provider set", result.stderr)

    def test_changed_provider_in_finished_archive_is_rejected(self) -> None:
        bundle, archive = self.make_archive_fixture()
        (bundle / "onnxruntime_providers_cuda.dll").write_bytes(b"changed source")
        result = self.run_archive_check(bundle, archive)
        self.assertNotEqual(result.returncode, 0)

    def test_provider_missing_from_bundle_is_rejected(self) -> None:
        bundle, archive = self.make_archive_fixture()
        (bundle / "onnxruntime_providers_cuda.dll").unlink()
        result = self.run_archive_check(bundle, archive)
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
