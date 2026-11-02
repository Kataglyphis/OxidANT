"""The Windows GPU candidate selects only the requested image/provider pairs."""

import ast
import json
import os
import re
import shutil
import subprocess
import sys
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
    preflight = content.split("\n  preflight:\n", 1)[-1].split("\n  candidate:\n", 1)[0]
    profile = content.split("\n  gpu-profile:\n", 1)[-1]
    return (
        "python -I scripts/windows/select_gpu_matrix.py" in preflight
        and "if ($LASTEXITCODE -ne 0) { throw 'Invalid GPU vendor selection.' }"
        in preflight
        and "matrix: ${{ steps.vendors.outputs.matrix }}" in preflight
        and "needs: preflight" in candidate
        and "matrix: ${{ fromJSON(needs.preflight.outputs.matrix) }}" in candidate
        and "matrix: ${{ fromJSON(needs.preflight.outputs.matrix) }}" in profile
        and "needs: [preflight, candidate]" in profile
        and "if: ${{ !cancelled() && inputs.verify-gpu && needs.preflight.result == 'success' }}"
        in profile
        and "continue-on-error" not in profile
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
            "scripts/windows/Get-ModelLockFile.ps1 -Suffix '.onnx'",
            'Join-Path $stage "resources/models/$model"',
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
    def search(self, pattern: str, text: str) -> re.Match[str]:
        match = re.search(pattern, text)
        if match is None:
            self.fail(f"{pattern!r} has no match")
        return match

    def test_linux_gpu_preflights_require_locked_file_api(self) -> None:
        for filename in ("linux-nvidia-gpu-build.yml", "linux-x64-amd-gpu.yml"):
            with self.subTest(filename=filename):
                text = (ROOT / ".github/workflows" / filename).read_text(
                    encoding="utf-8"
                )
                preflight = text.split("Check pinned GPU verification APIs", 1)[
                    1
                ].split("- name: Select", 1)[0]
                self.assertIn("shared/python/verify_locked_files.py", preflight)
                self.assertIn("fetch-data-lock.py", preflight)
                self.assertIn("grep -q '^def read_lock('", preflight)
                self.assertIn("grep -q '^def matches('", preflight)
                self.assertIn("exit 1", preflight)

    def test_reusable_candidate_inputs_are_default_off(self) -> None:
        def valid(content: str, group: str, windows: bool) -> bool:
            if "  workflow_call:\n" not in content:
                return False
            call = content.split("  workflow_call:\n", 1)[1].split("\npermissions:", 1)[
                0
            ]
            suffix = (
                "needs.preflight.outputs.suffix"
                if windows
                else "needs.select.outputs.suffix"
                if group == "linux-nvidia-gpu"
                else "needs.scope.outputs.suffix"
            )
            groups = re.findall(r"(?m)^\s+group: (.+)$", content)
            return (
                "      verify-gpu:\n        type: boolean\n        default: false"
                in call
                and "      GHCR_PAT:\n        required: false" in call
                and f"group: {group}-${{{{ github.ref }}}}-${{{{ github.workflow }}}}"
                in content
                and "      invocation-scope:\n        type: string\n        default: default"
                in call
                and "third_party/ANTfrastructure/shared/python/gpu_invocation_scope.py"
                in content
                and bool(groups)
                and all(
                    suffix in line and "inputs.invocation-scope" not in line
                    for line in groups
                )
                and (
                    "&& inputs.verify-gpu &&"
                    in content.split("\n  gpu-profile:\n", 1)[-1]
                    if windows or group == "linux-nvidia-gpu"
                    else "if: ${{ inputs.verify-gpu }}"
                    in content.split("\n  gpu-profile:\n", 1)[-1]
                )
                and (
                    not windows
                    or "      amd-device-index:\n        type: string\n        default: '0'"
                    in call
                )
            )

        workflows = {
            "linux-nvidia-gpu.yml": "linux-nvidia-gpu",
            "linux-x64-amd-gpu.yml": "linux-x64-amd-gpu",
            "windows-gpu-candidates.yml": "windows-gpu-candidates",
        }
        for filename, group in workflows.items():
            content = (ROOT / ".github/workflows" / filename).read_text(
                encoding="utf-8"
            )
            with self.subTest(workflow=filename):
                windows = filename.startswith("windows-")
                self.assertTrue(valid(content, group, windows))
                for original, replacement in (
                    ("  workflow_call:\n", "  workflow_disabled:\n"),
                    (
                        "      GHCR_PAT:\n        required: false",
                        "      WRONG_PAT:\n        required: false",
                    ),
                    ("${{ github.ref }}-${{ github.workflow }}", "${{ github.ref }}"),
                    ("outputs.suffix", "inputs.invocation-scope"),
                    (
                        "&& inputs.verify-gpu &&"
                        if filename != "linux-x64-amd-gpu.yml"
                        else "if: ${{ inputs.verify-gpu }}",
                        "if: true",
                    ),
                ):
                    self.assertFalse(
                        valid(content.replace(original, replacement), group, windows)
                    )

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
            destination = self.search(r"(?m)^\s+path: (.+)$", job).group(1)
            used = self.search(r"(?m)^\s+CANDIDATE_DIR: (.+)$", job).group(1)
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

    def test_scope_helper_rejects_unsafe_names_before_output(self) -> None:
        hub = Path(
            os.environ.get("ANTFRASTRUCTURE_DIR", ROOT / "third_party/ANTfrastructure")
        )
        helper = hub / "shared/python/gpu_invocation_scope.py"
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "output"
            for scope, expected in (
                ("default", ""),
                ("release-a", "-scope-release-a"),
                ("release-b", "-scope-release-b"),
                ("bad/name", None),
                ("Bad", None),
                ("", None),
                ("a" * 33, None),
            ):
                with self.subTest(scope=scope):
                    output.write_text("sentinel\n")
                    result = subprocess.run(
                        [sys.executable, "-I", str(helper)],
                        env={
                            **os.environ,
                            "GPU_INVOCATION_SCOPE": scope,
                            "GITHUB_OUTPUT": str(output),
                        },
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    if expected is None:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(output.read_text(), "sentinel\n")
                    else:
                        self.assertEqual(result.returncode, 0, result.stderr)
                        self.assertEqual(
                            output.read_text(), f"sentinel\nsuffix={expected}\n"
                        )

    def test_windows_profile_continues_per_vendor_after_sibling_failure(self) -> None:
        content = WORKFLOW.read_text(encoding="utf-8")
        self.assertTrue(matrix_is_valid(content))
        profile = content.split("\n  gpu-profile:\n", 1)[1]
        download = profile.split("actions/download-artifact@", 1)[1].split(
            "\n      - ", 1
        )[0]
        self.assertIn("name: unverified-oxidant-", download)
        self.assertNotIn("continue-on-error", download)
        self.assertNotIn("needs.candidate.result == 'success'", profile)
        condition = self.search(r"(?m)^    if: \$\{\{ (.+) \}\}$", profile)

        def evaluate(cancelled, verify, preflight, candidate):
            expression = condition.group(1)
            expression = expression.replace("!cancelled()", repr(not cancelled))
            expression = expression.replace("inputs.verify-gpu", repr(verify))
            expression = expression.replace("needs.preflight.result", repr(preflight))
            expression = expression.replace("needs.candidate.result", repr(candidate))
            expression = expression.replace("&&", " and ").replace("||", " or ")
            tree = ast.parse(expression, mode="eval")
            allowed = (
                ast.Expression,
                ast.BoolOp,
                ast.And,
                ast.Or,
                ast.Compare,
                ast.Eq,
                ast.Constant,
            )
            if any(not isinstance(node, allowed) for node in ast.walk(tree)):
                raise ValueError("unsupported workflow condition")
            return bool(
                eval(
                    compile(tree, "<gpu-profile-condition>", "eval"),
                    {"__builtins__": {}},
                    {},
                )
            )

        for candidate_result in ("success", "failure", "skipped"):
            with self.subTest(candidate_result=candidate_result):
                self.assertTrue(evaluate(False, True, "success", candidate_result))
        for cancelled, verify, preflight in (
            (True, True, "success"),
            (False, False, "success"),
            (False, True, "failure"),
        ):
            self.assertFalse(evaluate(cancelled, verify, preflight, "failure"))

    def test_vendor_selector_emits_only_supported_rows_and_rejects_invalid_input(
        self,
    ) -> None:
        selector = ROOT / "scripts/windows/select_gpu_matrix.py"
        expected = {
            "nvidia": ("winamd64-nvidia", "cuda", "gpu-nvidia-cuda"),
            "amd": ("winamd64-rocm", "directml", "gpu-amd-directml"),
        }
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "output"
            for vendor in ("all", "nvidia", "amd", "invalid", "AMD", ""):
                with self.subTest(vendor=vendor):
                    output.write_text("sentinel\n")
                    result = subprocess.run(
                        [sys.executable, "-I", str(selector)],
                        env={
                            **os.environ,
                            "GPU_VENDOR": vendor,
                            "GITHUB_OUTPUT": str(output),
                        },
                        capture_output=True,
                        text=True,
                        check=False,
                    )
                    if vendor not in {"all", "nvidia", "amd"}:
                        self.assertNotEqual(result.returncode, 0)
                        self.assertEqual(output.read_text(), "sentinel\n")
                        continue
                    self.assertEqual(result.returncode, 0, result.stderr)
                    lines = output.read_text().splitlines()
                    self.assertEqual(len(lines), 2)
                    rows = json.loads(lines[1].removeprefix("matrix="))["include"]
                    self.assertEqual(
                        {row["vendor"] for row in rows},
                        set(expected) if vendor == "all" else {vendor},
                    )
                    for row in rows:
                        self.assertEqual(
                            (row["tag"], row["provider"], row["runner"]),
                            expected[row["vendor"]],
                        )

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
                    match = self.search(rf"(?m)^\s+{option}: (.+)$", clean)
                    self.assertIn("env -u KATAGLYPHIS_ONNX_MODEL", match.group(1))
                    self.assertIn(
                        f'check-missing-gpu-runtime.sh" {vendor} {executable}',
                        match.group(1),
                    )
                self.assertIn("test-check-missing-gpu-runtime.sh", clean)

    def test_wrong_architecture_and_provider_are_rejected(self) -> None:
        content = WORKFLOW.read_text(encoding="utf-8")
        for mutant in (
            content.replace("needs: preflight", "needs: []"),
            content.replace(
                "fromJSON(needs.preflight.outputs.matrix)", "fromJSON('{}')"
            ),
            content.replace("!cancelled() && inputs.verify-gpu", "inputs.verify-gpu"),
            content.replace("needs: [preflight, candidate]", "needs: candidate"),
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
                f'scripts/linux/profile-gpu-packages.sh "$CANDIDATE_DIR" {device}',
                job,
            )

    @unittest.skipUnless(os.name == "nt", "runs a native Windows launcher fixture")
    def test_profile_preflight_isolates_overrides_and_rejects_missing_runtime(
        self,
    ) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8").split("\n  gpu-profile:\n", 1)[
            1
        ]
        start = '          if (-not (Test-Path -LiteralPath (Join-Path $stage "resources/models/$model")'
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
                f"$model = '{model.name}'\n"
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
            # A runner's TEMP may be an 8.3 path that the launcher's %~dp0 expands.
            selected = Path((stage / "selected-runtime.txt").read_text())
            self.assertEqual(selected.resolve(), runtime.resolve())
            runtime.unlink()
            self.assertIn("Bundled ONNX Runtime missing", run().stderr)
            runtime.touch()
            model.unlink()
            self.assertIn("Bundled model missing", run().stderr)
            model.touch()
            env["FIXTURE_FAIL"] = "1"
            self.assertIn("GPU inference or profiling failed", run().stderr)

    def test_nvidia_artifact_consumers_require_the_current_attempt(self) -> None:
        content = (ROOT / ".github/workflows/linux-nvidia-gpu.yml").read_text(
            encoding="utf-8"
        )
        build = (ROOT / ".github/workflows/linux-nvidia-gpu-build.yml").read_text(
            encoding="utf-8"
        )
        names = re.findall(
            r"(?m)^\s+(?:name|artifact): (oxidant-linux-\$\{\{ matrix\.arch \}\}-nvidia-host-runtime-candidate-[^\n]+)$",
            content,
        )
        self.assertEqual(len(names), 3)
        self.assertEqual(len(set(names)), 1)
        self.assertIn("github.run_attempt", names[0])
        self.assertEqual(
            build.count(
                "name: "
                + names[0]
                .replace("matrix.arch", "inputs.arch")
                .replace("needs.select.outputs.suffix", "steps.scope.outputs.suffix")
            ),
            2,
        )
        for upload in build.split("uses: actions/upload-artifact@")[1:]:
            self.assertIn("overwrite: false", upload)
        first = {
            names[0]
            .replace("${{ matrix.arch }}", arch)
            .replace("${{ github.run_id }}", "42")
            .replace("${{ github.run_attempt }}", "1"): "attempt-1"
            for arch in ("x64", "arm64")
        }
        self.assertEqual(len(first), 2)
        for arch in ("x64", "arm64"):
            artifact = (
                names[0]
                .replace("${{ matrix.arch }}", arch)
                .replace("${{ github.run_id }}", "42")
                .replace("${{ github.run_attempt }}", "2")
            )
            self.assertNotIn(artifact, first)
            first[artifact] = "attempt-2"
        self.assertEqual(set(first.values()), {"attempt-1", "attempt-2"})
        scoped = {
            names[0]
            .replace("${{ matrix.arch }}", "x64")
            .replace("${{ github.run_id }}", "42")
            .replace("${{ github.run_attempt }}", "1")
            .replace("${{ needs.select.outputs.suffix }}", suffix)
            for suffix in ("", "-scope-alpha", "-scope-beta")
        }
        self.assertEqual(len(scoped), 3)

    def test_amd_artifact_consumers_require_the_current_attempt(self) -> None:
        amd = (ROOT / ".github/workflows/linux-x64-amd-gpu.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn(
            "name: ${{ format('{0}-attempt-{1}{2}', steps.name.outputs.artifact_prefix, github.run_attempt, needs.scope.outputs.suffix) }}\n          overwrite: false",
            amd,
        )
        consumer = "${{ format('{0}-attempt-{1}{2}', needs.build.outputs.artifact_prefix, github.run_attempt, needs.scope.outputs.suffix) }}"
        self.assertIn(f"artifact: {consumer}", amd)
        self.assertIn(f"name: {consumer}", amd)
        self.assertIn("artifact_prefix: ${{ steps.name.outputs.artifact_prefix }}", amd)
        scoped = {
            f"oxidant-1-linux-x64-amd-host-runtime-candidate-attempt-1{suffix}"
            for suffix in ("", "-scope-alpha", "-scope-beta")
        }
        self.assertEqual(len(scoped), 3)

    def test_windows_profile_requires_the_current_attempt(self) -> None:
        windows = WORKFLOW.read_text(encoding="utf-8")
        name = "unverified-oxidant-${{ matrix.provider }}-${{ matrix.vendor }}-windows-x64-attempt-${{ github.run_attempt }}${{ needs.preflight.outputs.suffix }}"
        self.assertIn(f"name: {name}\n          overwrite: false", windows)
        self.assertEqual(windows.count(f"name: {name}"), 2)
        for provider, vendor in (("cuda", "nvidia"), ("directml", "amd")):
            first = (
                name.replace("${{ matrix.provider }}", provider)
                .replace("${{ matrix.vendor }}", vendor)
                .replace("${{ github.run_attempt }}", "1")
            )
            retry = (
                name.replace("${{ matrix.provider }}", provider)
                .replace("${{ matrix.vendor }}", vendor)
                .replace("${{ github.run_attempt }}", "2")
            )
            self.assertNotEqual(first, retry)
            scoped = {
                first.replace("${{ needs.preflight.outputs.suffix }}", suffix)
                for suffix in ("", "-scope-alpha", "-scope-beta")
            }
            self.assertEqual(len(scoped), 3)


if __name__ == "__main__":
    unittest.main()
