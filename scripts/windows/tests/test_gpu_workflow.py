"""The Windows GPU candidate selects only the requested image/provider pairs."""

import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

WORKFLOW = (
    Path(__file__).resolve().parents[3] / ".github/workflows/windows-gpu-candidates.yml"
)
ROOT = Path(__file__).resolve().parents[3]


def matrix_is_valid(content: str) -> bool:
    candidate = content.split("\n  candidate:\n", 1)[-1].split("\n  gpu-profile:\n", 1)[
        0
    ]
    rows = re.findall(
        r"(?m)^          - vendor: (\S+)\n            tag: (\S+)\n            provider: (\S+)$",
        candidate,
    )
    return (
        len(rows) == 2
        and set(rows)
        == {
            ("nvidia", "winamd64-nvidia", "cuda"),
            ("amd", "winamd64-rocm", "directml"),
        }
        and candidate.count("- vendor:") == 2
        and "needs: preflight" in candidate
        and candidate.index("CPU inference from the finished ZIP")
        < candidate.index("Upload the unverified candidate")
        and "unverified-oxidant-${{ matrix.provider }}-${{ matrix.vendor }}-windows-x64"
        in candidate
    )


def directml_identity_is_checked(content: str) -> bool:
    """The profiled DirectML session and AMD hardware check use the same index."""
    return all(
        value in content
        for value in (
            "DIRECTML_INDEX: ${{ inputs.amd-device-index }}",
            "verify-dxgi-adapter.py --vendor amd --index $env:DIRECTML_INDEX",
            "$env:KATAGLYPHIS_DIRECTML_DEVICE_ID = $env:DIRECTML_INDEX",
        )
    )


def windows_profile_uses_candidate_launcher(content: str) -> bool:
    """Profile the package's default launcher and model, not host overrides."""
    job = content.split("\n  gpu-profile:\n", 1)[-1]
    return all(
        value in job
        for value in (
            "resources/models/deim_dfine_n_coco.onnx",
            "Remove-Item Env:\\KATAGLYPHIS_ONNX_MODEL, Env:\\KATAGLYPHIS_ORT_DEVICE",
            "Env:\\KATAGLYPHIS_ONNX_BACKEND, Env:\\ORT_DYLIB_PATH",
            "Join-Path $stage 'onnxruntime.dll'",
            'Join-Path $stage "oxidant-$env:PROVIDER.cmd"',
            "--profile (Join-Path $stage 'ort-profile')",
            "--device $env:PROVIDER",
        )
    ) and not re.search(r"(?im)^\s*\$env:ORT_DYLIB_PATH\s*=", job)


def write_native_profile_stub(stage: Path) -> None:
    """Use the real launcher with a synthetic executable; no OxidANT build runs."""
    shutil.copyfile(
        ROOT / "packaging/windows/oxidant-cuda.cmd", stage / "oxidant-cuda.cmd"
    )
    source = stage / "profile-stub.cs"
    source.write_text(
        "using System; using System.IO; using System.Reflection;\n"
        "class ProfileStub { static int Main() {\n"
        'if (Environment.GetEnvironmentVariable("ORT_DYLIB_PATH") != null ||\n'
        '    Environment.GetEnvironmentVariable("KATAGLYPHIS_ONNX_MODEL") != null ||\n'
        '    Environment.GetEnvironmentVariable("KATAGLYPHIS_ONNX_BACKEND") != null ||\n'
        '    Environment.GetEnvironmentVariable("KATAGLYPHIS_ORT_DEVICE") != "cuda") return 71;\n'
        'if (Environment.GetEnvironmentVariable("FIXTURE_FAIL") != null) return 76;\n'
        "string dir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);\n"
        'string dll = Path.Combine(dir, "onnxruntime.dll");\n'
        "if (!File.Exists(dll)) return 75;\n"
        'File.WriteAllText(Path.Combine(dir, "selected-runtime.txt"), dll);\n'
        "return 0; } }\n",
        encoding="utf-8",
    )
    compiler = (
        Path(os.environ["SystemRoot"]) / "Microsoft.NET/Framework64/v4.0.30319/csc.exe"
    )
    subprocess.run(
        [
            str(compiler),
            "/nologo",
            f"/out:{stage / 'kataglyphis_cli.exe'}",
            str(source),
        ],
        capture_output=True,
        text=True,
        check=True,
    )


class GpuWorkflow(unittest.TestCase):
    @unittest.skipUnless(os.name == "nt", "runs a native Windows launcher fixture")
    def test_candidate_zip_smoke_requires_inference_and_media(self) -> None:
        job = (
            WORKFLOW.read_text(encoding="utf-8")
            .split("CPU inference from the finished ZIP", 1)[1]
            .split("Upload the unverified candidate", 1)[0]
        )
        start = "          if (-not (Test-Path -LiteralPath (Join-Path $stage 'lib/gstreamer-1.0')"
        end = "          if ($LASTEXITCODE -ne 0) { throw 'Bundled media pipeline from the candidate failed' }"
        snippet = start + job.split(start, 1)[1].split(end, 1)[0] + end
        with tempfile.TemporaryDirectory() as temp:
            stage = Path(temp) / "candidate"
            stage.mkdir()
            plugins = stage / "lib/gstreamer-1.0"
            plugins.mkdir(parents=True)
            (stage / "oxidant-cuda.cmd").write_text(
                "@echo off\n"
                'if "%~1"=="onnx-check" if defined FIXTURE_FAIL_INFERENCE exit /b 71\n'
                'if "%~1"=="media-check" if defined FIXTURE_FAIL_MEDIA exit /b 72\n'
                "exit /b 0\n",
                encoding="ascii",
            )
            script = Path(temp) / "fixture.ps1"
            script.write_text(
                "$ErrorActionPreference = 'Stop'\n$stage = $env:FIXTURE_STAGE\n"
                + snippet
                + "\n",
                encoding="utf-8",
            )
            env = dict(os.environ, FIXTURE_STAGE=str(stage), PROVIDER="cuda")
            powershell = (
                Path(os.environ["SystemRoot"])
                / "System32/WindowsPowerShell/v1.0/powershell.exe"
            )

            def run(failure: str = "") -> subprocess.CompletedProcess[str]:
                child = env.copy()
                if failure:
                    child[failure] = "1"
                return subprocess.run(
                    [
                        str(powershell),
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                        str(script),
                    ],
                    env=child,
                    capture_output=True,
                    text=True,
                    check=False,
                )

            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(
                "CPU inference from the candidate failed",
                run("FIXTURE_FAIL_INFERENCE").stderr,
            )
            self.assertIn(
                "Bundled media pipeline from the candidate failed",
                run("FIXTURE_FAIL_MEDIA").stderr,
            )
            plugins.rmdir()
            self.assertIn("Bundled GStreamer plugins missing", run().stderr)

    def test_profile_downloads_ignore_stale_candidates(self) -> None:
        workflows = (
            (
                ROOT / ".github/workflows/linux-nvidia-gpu.yml",
                '"$CANDIDATE_DIR"',
                "arch",
            ),
            (
                ROOT / ".github/workflows/linux-x64-amd-gpu.yml",
                '"$CANDIDATE_DIR"',
                None,
            ),
            (WORKFLOW, "$env:CANDIDATE_DIR", "vendor"),
        )
        for workflow, reference, matrix_key in workflows:
            job = workflow.read_text(encoding="utf-8").split("\n  gpu-profile:\n", 1)[1]
            destination = re.search(r"(?m)^\s+path: (.+)$", job).group(1)
            used = re.search(r"(?m)^\s+CANDIDATE_DIR: (.+)$", job).group(1)
            self.assertEqual(destination, used)
            self.assertIn(reference, job)
            for key in ("github.run_id", "github.run_attempt"):
                self.assertIn("${{ " + key + " }}", destination)
            if matrix_key:
                self.assertIn("${{ matrix." + matrix_key + " }}", destination)
            with tempfile.TemporaryDirectory() as temp:

                def path(run, attempt, template=destination):
                    return Path(
                        template.replace("${{ runner.temp }}", temp)
                        .replace("${{ github.run_id }}", run)
                        .replace("${{ github.run_attempt }}", attempt)
                        .replace("${{ matrix.arch }}", "x64")
                        .replace("${{ matrix.vendor }}", "amd")
                    )

                stale, previous, current = (
                    path("1", "1"),
                    path("2", "1"),
                    path("2", "2"),
                )
                for directory in (stale, previous, current):
                    directory.mkdir()
                (stale / "old.tar.gz").touch()
                (previous / "previous.tar.gz").touch()
                (current / "current.tar.gz").touch()
                self.assertEqual(
                    [p.name for p in current.glob("*.tar.gz")], ["current.tar.gz"]
                )

    def test_supported_matrix(self) -> None:
        self.assertTrue(matrix_is_valid(WORKFLOW.read_text(encoding="utf-8")))

    def test_linux_clean_host_checks_missing_runtime_for_each_format(self) -> None:
        """A CPU-default TAR or AppImage must not pass as a GPU candidate."""
        for filename, vendor in (
            ("linux-nvidia-gpu.yml", "nvidia"),
            ("linux-x64-amd-gpu.yml", "amd"),
        ):
            with self.subTest(vendor=vendor):
                text = (ROOT / ".github/workflows" / filename).read_text(
                    encoding="utf-8"
                )
                clean = text.split("\n  clean-host:\n", 1)[1].split(
                    "\n  gpu-profile:\n", 1
                )[0]
                for option, executable in (
                    ("deb-command", "oxidant"),
                    ("tarball-command", "./oxidant"),
                    ("appimage-command", '"$APPIMAGE"'),
                ):
                    match = re.search(rf"(?m)^\s+{option}: (.+)$", clean)
                    self.assertIsNotNone(match)
                    self.assertIn("env -u KATAGLYPHIS_ONNX_MODEL", match.group(1))
                    self.assertIn(
                        f'check-missing-gpu-runtime.sh" {vendor} {executable}',
                        match.group(1),
                    )
                self.assertIn("test-check-missing-gpu-runtime.sh", clean)

    def test_wrong_architecture_and_provider_are_rejected(self) -> None:
        content = WORKFLOW.read_text(encoding="utf-8")
        for mutant in (
            content.replace("winamd64-nvidia", "winarm64-nvidia"),
            content.replace("winamd64-rocm", "winarm64-rocm"),
            content.replace("provider: cuda", "provider: directml", 1),
            content.replace("needs: preflight", "needs: []"),
        ):
            self.assertFalse(matrix_is_valid(mutant))

    def test_directml_hardware_and_session_use_the_same_index(self) -> None:
        content = WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(directml_identity_is_checked(content))
        self.assertFalse(
            directml_identity_is_checked(
                content.replace(
                    "$env:KATAGLYPHIS_DIRECTML_DEVICE_ID = $env:DIRECTML_INDEX", ""
                )
            )
        )

    def test_profile_uses_the_delivered_launcher_and_model(self) -> None:
        content = WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(windows_profile_uses_candidate_launcher(content))
        for original, replacement in (
            (
                'Join-Path $stage "oxidant-$env:PROVIDER.cmd"',
                "Join-Path $stage 'kataglyphis_cli.exe'",
            ),
            ("Env:\\KATAGLYPHIS_ONNX_MODEL, ", ""),
            ("Env:\\KATAGLYPHIS_ORT_DEVICE", "Env:\\UNRELATED"),
            ("Env:\\KATAGLYPHIS_ONNX_BACKEND", "Env:\\UNRELATED_BACKEND"),
            ("Env:\\ORT_DYLIB_PATH", "Env:\\UNRELATED_RUNTIME"),
            ("Join-Path $stage 'onnxruntime.dll'", "Join-Path $stage 'unrelated.dll'"),
        ):
            with self.subTest(original=original):
                self.assertFalse(
                    windows_profile_uses_candidate_launcher(
                        content.replace(original, replacement)
                    )
                )
        self.assertFalse(
            windows_profile_uses_candidate_launcher(
                content.replace(
                    '& (Join-Path $stage "oxidant-$env:PROVIDER.cmd")',
                    '$env:ORT_DYLIB_PATH = "host.dll"\n          & (Join-Path $stage "oxidant-$env:PROVIDER.cmd")',
                )
            )
        )
        for filename, device in (
            ("linux-nvidia-gpu.yml", "cuda"),
            ("linux-x64-amd-gpu.yml", "migraphx"),
        ):
            job = (
                (ROOT / ".github/workflows" / filename)
                .read_text(encoding="utf-8")
                .split("\n  gpu-profile:\n", 1)[1]
            )
            self.assertIn(
                f'scripts/linux/profile-gpu-bundle.sh "$work/oxidant" {device} "$work/ort"',
                job,
            )

    @unittest.skipUnless(os.name == "nt", "runs a native Windows launcher fixture")
    def test_profile_preflight_isolates_overrides_and_rejects_missing_runtime(
        self,
    ) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8").split("\n  gpu-profile:\n", 1)[
            1
        ]
        start = "          if (-not (Test-Path -LiteralPath (Join-Path $stage 'resources/models/deim_dfine_n_coco.onnx')"
        end = "          if ($LASTEXITCODE -ne 0) { throw 'GPU inference or profiling failed' }"
        snippet = start + workflow.split(start, 1)[1].split(end, 1)[0] + end
        with tempfile.TemporaryDirectory() as temp:
            stage = Path(temp) / "candidate"
            model = stage / "resources/models/deim_dfine_n_coco.onnx"
            model.parent.mkdir(parents=True)
            model.touch()
            runtime = stage / "onnxruntime.dll"
            runtime.touch()
            write_native_profile_stub(stage)
            script = Path(temp) / "fixture.ps1"
            script.write_text(
                "$ErrorActionPreference = 'Stop'\n$stage = $env:FIXTURE_STAGE\n"
                + snippet
                + "\n",
                encoding="utf-8",
            )
            env = dict(os.environ, FIXTURE_STAGE=str(stage), PROVIDER="cuda")
            for key in (
                "ORT_DYLIB_PATH",
                "KATAGLYPHIS_ONNX_MODEL",
                "KATAGLYPHIS_ONNX_BACKEND",
                "KATAGLYPHIS_ORT_DEVICE",
            ):
                env[key] = str(Path(temp) / "host-runtime")
            powershell = (
                Path(os.environ["SystemRoot"])
                / "System32/WindowsPowerShell/v1.0/powershell.exe"
            )

            def run():
                return subprocess.run(
                    [
                        str(powershell),
                        "-NoProfile",
                        "-NonInteractive",
                        "-ExecutionPolicy",
                        "Bypass",
                        "-File",
                        str(script),
                    ],
                    env=env,
                    capture_output=True,
                    text=True,
                    check=False,
                )

            result = run()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((stage / "selected-runtime.txt").read_text(), str(runtime))
            runtime.unlink()
            self.assertIn("Bundled ONNX Runtime missing", run().stderr)
            runtime.touch()
            model.unlink()
            self.assertIn("Bundled model missing", run().stderr)
            model.touch()
            env["FIXTURE_FAIL"] = "1"
            self.assertIn("GPU inference or profiling failed", run().stderr)

    def test_nvidia_artifacts_survive_partial_and_full_reruns(self) -> None:
        content = (ROOT / ".github/workflows/linux-nvidia-gpu.yml").read_text(
            encoding="utf-8"
        )
        names = re.findall(
            r"(?m)^\s+(?:name|artifact): (oxidant-linux-\$\{\{ matrix\.arch \}\}-nvidia-host-runtime-candidate-[^\n]+)$",
            content,
        )
        self.assertEqual(len(names), 3)
        self.assertEqual(len(set(names)), 1)
        self.assertNotIn("github.run_attempt", names[0])
        self.assertIn(
            "overwrite: true",
            content.split("Upload unverified candidate", 1)[1].split(
                "\n  clean-host:", 1
            )[0],
        )
        first = {
            names[0]
            .replace("${{ matrix.arch }}", arch)
            .replace("${{ github.run_id }}", "42"): "attempt-1"
            for arch in ("x64", "arm64")
        }
        self.assertEqual(len(first), 2)
        for arch in ("x64", "arm64"):
            artifact = (
                names[0]
                .replace("${{ matrix.arch }}", arch)
                .replace("${{ github.run_id }}", "42")
            )
            self.assertIn(
                artifact, first
            )  # Rerunning only a consumer retains the successful build.
            first[artifact] = (
                "attempt-2"  # Rerunning all jobs replaces each matrix artifact.
            )
        self.assertEqual(set(first.values()), {"attempt-2"})

    def test_stable_amd_and_windows_uploads_allow_full_reruns(self) -> None:
        amd = (ROOT / ".github/workflows/linux-x64-amd-gpu.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "name: ${{ steps.name.outputs.artifact }}\n          overwrite: true", amd
        )
        self.assertIn("artifact: ${{ needs.build.outputs.artifact }}", amd)
        self.assertIn("name: ${{ needs.build.outputs.artifact }}", amd)
        windows = WORKFLOW.read_text(encoding="utf-8")
        name = (
            "unverified-oxidant-${{ matrix.provider }}-${{ matrix.vendor }}-windows-x64"
        )
        self.assertIn(f"name: {name}\n          overwrite: true", windows)
        self.assertEqual(windows.count(f"name: {name}"), 2)


if __name__ == "__main__":
    unittest.main()
