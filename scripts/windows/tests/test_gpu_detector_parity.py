#!/usr/bin/env python3
"""Exercise the Windows ZIP parity orchestration with real fake subprocesses."""

import hashlib
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
HUB = Path(os.environ.get("PARITY_TEST_HUB", ROOT / "third_party/ANTfrastructure"))
spec = importlib.util.spec_from_file_location(
    "parity", ROOT / "scripts/windows/check_gpu_detector_parity.py"
)
if spec is None or spec.loader is None:
    raise ImportError("cannot load check_gpu_detector_parity.py")
parity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(parity)

FAKE = r"""
import json, os, pathlib, sys
args = sys.argv[1:]
out = pathlib.Path(args[args.index("--output") + 1])
profile = pathlib.Path(args[args.index("--profile") + 1] + ".json")
device = os.environ["KATAGLYPHIS_ORT_DEVICE"]
label = out.stem
for name in ("KATAGLYPHIS_PREPROCESS", "KATAGLYPHIS_SWAP_XY", "KATAGLYPHIS_ONNX_MODEL", "ORT_LIB_LOCATION", "ORT_MIGRAPHX_FP16_ENABLE", "MIGRAPHX_POISON"):
    assert name not in os.environ, name
assert pathlib.Path(os.environ["ORT_DYLIB_PATH"]).name == "onnxruntime.dll"
if device == "directml": assert os.environ["KATAGLYPHIS_DIRECTML_DEVICE_ID"] == "3"
else: assert "KATAGLYPHIS_DIRECTML_DEVICE_ID" not in os.environ
if os.getenv("FAKE_FAIL") == label: sys.exit(4)
reference = {"schema_version": 1, "width": 100, "height": 100, "detections": [
    {"class": 15, "score": 0.9, "box": [10,60,30,95]},
    {"class": 15, "score": 0.7, "box": [50,20,80,60]}]}
if os.getenv("FAKE_MISMATCH") == label: reference["detections"][0]["score"] = 0.4
if os.getenv("FAKE_REPEAT_DRIFT") == label: reference["detections"][0]["score"] = 0.94
if os.getenv("FAKE_LOW_GPU") == "1": reference["detections"][0]["score"] = 0.81 if label == "cpu" else 0.78
out.write_text(json.dumps(reference))
if "--repeat-output" in args and os.getenv("FAKE_NO_REPEAT") != label:
    repeated = json.loads(json.dumps(reference))
    if os.getenv("FAKE_REPEAT_MISMATCH") == label: repeated["detections"][0]["score"] = 0.4
    if os.getenv("FAKE_REPEAT_DRIFT") == label: repeated["detections"][0]["score"] = 0.86
    if os.getenv("FAKE_REPEAT_CROSS_DRIFT") == "1" and label in ("gpu1", "gpu2"): repeated["detections"][0]["score"] = 0.86 if label == "gpu1" else 0.94
    if os.getenv("FAKE_LOW_GPU") == "1" and label != "cpu": repeated["detections"][0]["score"] = 0.77
    pathlib.Path(args[args.index("--repeat-output") + 1]).write_text(json.dumps(repeated))
provider = {"cpu": "CPUExecutionProvider", "cuda": "CUDAExecutionProvider", "directml": "DmlExecutionProvider"}[device]
if os.getenv("FAKE_FALLBACK") == label: provider = "CPUExecutionProvider"
if os.getenv("FAKE_NO_PROFILE") != label:
    events = []
    for n in range(2 if "--repeat-output" in args else 1):
        events.append({"cat": "Session", "name": "model_run", "pid": 1, "tid": 1, "ts": n * 100, "dur": 90, "ph": "X"})
        if not (n == 1 and os.getenv("FAKE_SECOND_MISSING") == label):
            events.append({"cat": "Node", "name": "conv", "pid": 1, "tid": 1, "ts": n * 100 + 10, "dur": 20, "ph": "X", "args": {"provider": "CPUExecutionProvider" if n == 1 and os.getenv("FAKE_SECOND_FALLBACK") == label else provider, "op_name": "Conv"}})
    profile.write_text(json.dumps(events))
"""


class ParityTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.repo = self.root / "repo"
        self.stage = self.root / "stage"
        self.stage.mkdir()
        self.fake = self.root / "fake.py"
        self.fake.write_text(FAKE)
        for relative in (
            "shared/python/verify_locked_files.py",
            "shared/python/compare_detector_parity.py",
            "shared/python/verify_ort_profile.py",
            "linux/scripts/lib/fetch-data-lock.py",
        ):
            target = self.repo / "third_party/ANTfrastructure" / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(HUB / relative, target)
        for relative in (
            "scripts/verify-ort-gpu-profile.py",
            "crates/inference/tests/fixtures/two_cats.jpg",
        ):
            target = self.repo / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / relative, target)
        dxgi = (
            self.repo
            / "third_party/ANTfrastructure/windows/scripts/diagnostics/verify-dxgi-adapter.py"
        )
        dxgi.parent.mkdir(parents=True)
        dxgi.write_text(
            'import sys\nsys.exit(0 if sys.argv[1:] == ["--vendor", "amd", "--index", "3"] else 1)\n'
        )
        models = self.stage / "resources/models"
        models.mkdir(parents=True)
        lines = []
        for name in parity.MODEL_FILES:
            data = name.encode()
            (models / name).write_bytes(data)
            lines.append(
                f"{hashlib.sha256(data).hexdigest()} {name} https://example.invalid/{name}\n"
            )
        lock = self.repo / "resources/models/models.lock"
        lock.parent.mkdir(parents=True)
        lock.write_text("".join(lines))
        for name in ("kataglyphis_cli.exe", "onnxruntime.dll"):
            (self.stage / name).write_text("fixture")
        self.command = patch.object(
            parity,
            "detector_command",
            side_effect=lambda exe, fixture, output, profile, repeat: (
                [
                    sys.executable,
                    "-I",
                    str(self.fake),
                    "--image",
                    str(fixture),
                    "--output",
                    str(output),
                    "--profile",
                    str(profile),
                ]
                + (["--repeat-output", str(repeat)] if repeat else [])
            ),
        )
        self.command.start()
        self.addCleanup(self.command.stop)

    def check(self, device="directml", index="3"):
        return parity.check(self.stage, device, index, self.repo, self.root)

    def test_real_subprocesses_use_three_profiles_and_clean_environment(self):
        poisoned = {
            key: "poison"
            for key in (
                "KATAGLYPHIS_PREPROCESS",
                "KATAGLYPHIS_SWAP_XY",
                "KATAGLYPHIS_ONNX_MODEL",
                "KATAGLYPHIS_DIRECTML_DEVICE_ID",
                "ORT_LIB_LOCATION",
                "ORT_MIGRAPHX_FP16_ENABLE",
                "MIGRAPHX_POISON",
            )
        }
        poisoned["ANTFRASTRUCTURE_DIR"] = str(self.root / "untrusted-hub")
        with patch.dict(os.environ, poisoned):
            for device in ("directml", "cuda"):
                work = self.check(device)
                self.assertEqual(
                    sorted(p.name for p in work.glob("*profile*.json")),
                    ["cpu-profile.json", "gpu1-profile.json", "gpu2-profile.json"],
                )
                self.assertEqual(os.environ["KATAGLYPHIS_PREPROCESS"], "poison")

    def test_each_gpu_failure_fallback_and_parity_mismatch_blocks(self):
        for flag in (
            "FAKE_FAIL",
            "FAKE_FALLBACK",
            "FAKE_NO_PROFILE",
            "FAKE_MISMATCH",
            "FAKE_NO_REPEAT",
            "FAKE_REPEAT_MISMATCH",
            "FAKE_SECOND_FALLBACK",
        ):
            for label in ("gpu1", "gpu2"):
                with (
                    self.subTest(flag=flag, label=label),
                    patch.dict(os.environ, {flag: label}),
                    self.assertRaises((ValueError, subprocess.SubprocessError)),
                ):
                    self.check()
        for flag in (
            "FAKE_FAIL",
            "FAKE_NO_PROFILE",
            "FAKE_NO_REPEAT",
            "FAKE_REPEAT_MISMATCH",
            "FAKE_SECOND_MISSING",
        ):
            with (
                self.subTest(flag=flag, label="cpu"),
                patch.dict(os.environ, {flag: "cpu"}),
                self.assertRaises((ValueError, subprocess.SubprocessError)),
            ):
                self.check()

    def test_lock_fixture_helper_and_adapter_fail_closed(self):
        for name in parity.MODEL_FILES:
            path = self.stage / "resources/models" / name
            original = path.read_bytes()
            path.write_bytes(b"changed")
            with (
                self.subTest(name=name),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                self.check()
            path.write_bytes(original)
        fixture = self.repo / "crates/inference/tests/fixtures/two_cats.jpg"
        fixture.write_bytes(b"changed")
        with self.assertRaises(ValueError):
            self.check()

        shutil.copyfile(ROOT / "crates/inference/tests/fixtures/two_cats.jpg", fixture)
        with self.assertRaises(subprocess.CalledProcessError):
            self.check(index="2")
        for invalid in ("-1", "03", "x", "2147483648"):
            with self.subTest(index=invalid), self.assertRaises(ValueError):
                self.check(index=invalid)
        (
            self.repo
            / "third_party/ANTfrastructure/shared/python/compare_detector_parity.py"
        ).unlink()
        with self.assertRaises(ValueError):
            self.check()

    def test_gpu_repeats_must_agree_with_each_other(self):
        # Each GPU score remains within CPU tolerance; the two GPU scores do not.
        for label in ("gpu1", "gpu2"):
            with (
                self.subTest(label=label),
                patch.dict(os.environ, {"FAKE_REPEAT_DRIFT": label}),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                self.check()
        with (
            patch.dict(os.environ, {"FAKE_REPEAT_CROSS_DRIFT": "1"}),
            self.assertRaises(subprocess.CalledProcessError),
        ):
            self.check()

    def test_pairwise_gpu_reference_does_not_reapply_cpu_score_oracle(self):
        with patch.dict(os.environ, {"FAKE_LOW_GPU": "1"}):
            self.check()

    def test_production_command_executes_packaged_binary_directly(self):
        self.command.stop()
        exe = self.stage / "kataglyphis_cli.exe"
        command = parity.detector_command(
            exe, Path("fixture"), Path("output"), Path("profile"), Path("repeat")
        )
        self.assertEqual(
            command,
            [
                str(exe),
                "onnx-detect",
                "--image",
                "fixture",
                "--output",
                "output",
                "--profile",
                "profile",
                "--repeat-output",
                "repeat",
            ],
        )


def workflow_requires_parity(content):
    job = content.split("\n  gpu-profile:\n", 1)[-1]
    command = "python -I scripts/windows/check_gpu_detector_parity.py $stage --device $env:PROVIDER --directml-index $env:DIRECTML_INDEX"
    guard = "if ($LASTEXITCODE -ne 0) { throw 'Windows ZIP detector parity or per-run GPU profiling failed' }"
    return (
        "if: ${{ !cancelled() && inputs.verify-gpu && needs.preflight.result == 'success' }}"
        in job
        and "name: unverified-oxidant-${{ matrix.provider }}-${{ matrix.vendor }}-windows-x64-attempt-${{ github.run_attempt }}${{ needs.preflight.outputs.suffix }}"
        in job
        and f"          {command}\n          {guard}" in job
    )


class WorkflowContractTest(unittest.TestCase):
    def test_hardware_gate_is_opt_in_and_fail_closed(self):
        content = (ROOT / ".github/workflows/windows-gpu-candidates.yml").read_text()
        self.assertTrue(workflow_requires_parity(content))
        for before, after in (
            ("!cancelled() && inputs.verify-gpu", "always()"),
            (
                "python -I scripts/windows/check_gpu_detector_parity.py",
                "echo scripts/windows/check_gpu_detector_parity.py",
            ),
            (
                "if ($LASTEXITCODE -ne 0) { throw 'Windows ZIP detector parity or per-run GPU profiling failed' }",
                "if ($false) { throw 'ignored' }",
            ),
        ):
            with self.subTest(before=before):
                mutated = content.replace(before, after)
                self.assertNotEqual(mutated, content)
                self.assertFalse(workflow_requires_parity(mutated))


if __name__ == "__main__":
    unittest.main()
