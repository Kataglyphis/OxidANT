#!/usr/bin/env python3
"""Check the pinned shared parity comparator is required by the GPU workflows."""

import os
import re
import shlex
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


class ParityWiringTest(unittest.TestCase):
    @unittest.skipIf(os.name == "nt", "the producer preflight runs in Linux Bash")
    def test_profile_interval_api_is_required_before_pull(self):
        for filename in ("linux-x64-amd-gpu.yml", "linux-nvidia-gpu-build.yml"):
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                content = (ROOT / ".github/workflows" / filename).read_text()
                preflight = content.split(
                    "name: Check pinned GPU verification APIs", 1
                )[1].split("\n      - name: Select", 1)[0]
                assignment = next(
                    line.strip()
                    for line in preflight.splitlines()
                    if line.strip().startswith("ort_profile=")
                )
                guard = next(
                    line.strip()
                    for line in preflight.splitlines()
                    if line.strip().startswith('python3 -I "$ort_profile" --help')
                )
                helper = (
                    root
                    / "third_party/ANTfrastructure/shared/python/verify_ort_profile.py"
                )
                helper.parent.mkdir(parents=True)
                sentinel = root / "pull-reached"

                def run(
                    check=guard, assignment=assignment, sentinel=sentinel, root=root
                ):
                    sentinel.unlink(missing_ok=True)
                    return subprocess.run(
                        [
                            "bash",
                            "-c",
                            f"set -euo pipefail\n{assignment}\n{check}\ntouch {shlex.quote(str(sentinel))}",
                        ],
                        cwd=root,
                        capture_output=True,
                        check=False,
                    )

                self.assertNotEqual(run().returncode, 0)
                helper.write_text("print('usage: legacy helper')\n")
                self.assertNotEqual(run().returncode, 0)
                self.assertFalse(sentinel.exists())
                mutant = guard.replace("exit 1", "true")
                self.assertNotEqual(mutant, guard)
                self.assertEqual(run(mutant).returncode, 0)
                self.assertTrue(sentinel.exists())
                helper.write_text("print('--min-runs')\n")
                self.assertEqual(run().returncode, 0)

    @unittest.skipIf(os.name == "nt", "the producer preflight runs in Linux Bash")
    def test_producer_preflight_fails_if_shared_suite_is_missing_or_fails(self):
        for filename in ("linux-x64-amd-gpu.yml", "linux-nvidia-gpu-build.yml"):
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                content = (ROOT / ".github/workflows" / filename).read_text()
                preflight = content.split(
                    "name: Check pinned GPU verification APIs", 1
                )[1].split("\n      - name: Select", 1)[0]
                for token in (
                    "third_party/ANTfrastructure/shared/python/compare_detector_parity.py",
                    "third_party/ANTfrastructure/shared/python/tests/test_compare_detector_parity.py",
                    'test -f "$parity_comparator"',
                    'test -f "$parity_tests"',
                    'python3 -I "$parity_tests"',
                    "exit 1",
                    "set -euo pipefail",
                ):
                    self.assertIn(token, preflight)
                source = root / "third_party/ANTfrastructure/shared/python"
                (source / "tests").mkdir(parents=True)
                comparator = source / "compare_detector_parity.py"
                suite = source / "tests/test_compare_detector_parity.py"
                sentinel = root / "pull-reached"
                assignments = re.search(
                    r"(?m)^          parity_comparator=.*\n          parity_tests=.*$",
                    preflight,
                )
                check = re.search(
                    r'(?ms)^          if ! test -f "\$parity_comparator".*?^          fi$',
                    preflight,
                )
                if assignments is None or check is None:
                    self.fail("parity comparator guard is missing")
                guard = assignments.group(0) + "\n" + check.group(0)
                suite_lines = [
                    line.strip()
                    for line in preflight.splitlines()
                    if line.strip().startswith('python3 -I "$parity_tests"')
                ]
                self.assertEqual(suite_lines, ['python3 -I "$parity_tests"'])
                strict = next(
                    line.strip()
                    for line in preflight.splitlines()
                    if line.strip() == "set -euo pipefail"
                )

                def run(
                    actual_guard=guard,
                    actual_suite=suite_lines[0],
                    sentinel=sentinel,
                    strict=strict,
                    root=root,
                ):
                    sentinel.unlink(missing_ok=True)
                    program = "\n".join(
                        (
                            strict,
                            actual_guard,
                            actual_suite,
                            f"touch {shlex.quote(str(sentinel))}",
                        )
                    )
                    return subprocess.run(
                        ["bash", "-c", program],
                        cwd=root,
                        capture_output=True,
                        check=False,
                    )

                self.assertNotEqual(run().returncode, 0)
                comparator.touch()
                self.assertNotEqual(run().returncode, 0)
                suite.write_text("raise SystemExit(1)\n")
                self.assertNotEqual(run().returncode, 0)
                self.assertFalse(sentinel.exists())
                self.assertEqual(
                    run(actual_suite=suite_lines[0] + " || true").returncode, 0
                )
                self.assertTrue(sentinel.exists())
                suite.write_text("raise SystemExit(0)\n")
                self.assertEqual(run().returncode, 0)
                self.assertTrue(sentinel.exists())
                comparator.unlink()
                mutant = guard.replace("; exit 1", "; true", 1)
                self.assertNotEqual(mutant, guard)
                self.assertNotEqual(run().returncode, 0)
                self.assertFalse(sentinel.exists())
                self.assertEqual(run(actual_guard=mutant).returncode, 0)
                self.assertTrue(sentinel.exists())

    @unittest.skipIf(os.name == "nt", "the GPU command runs in Linux Bash")
    def test_promotion_command_overrides_poisoned_inherited_hub(self):
        for filename in ("linux-x64-amd-gpu.yml", "linux-nvidia-gpu.yml"):
            with self.subTest(filename=filename), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                content = (ROOT / ".github/workflows" / filename).read_text()
                job = content.split("name: Profile immutable", 1)[1]
                match = re.search(r"(?m)^          command: (.+)$", job)
                if match is None:
                    self.fail("promotion command is missing")
                command = match.group(1)
                bin_dir = root / "bin"
                bin_dir.mkdir()
                fake_bash = bin_dir / "bash"
                fake_bash.write_text(
                    '#!/bin/sh\nprintf "%s\\n" "$ANTFRASTRUCTURE_DIR" > "$OBSERVED_HUB"\n'
                )
                fake_bash.chmod(0o755)
                observed = root / "observed"
                workspace = root / "workspace with spaces"
                env = {
                    **os.environ,
                    "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"],
                    "GITHUB_WORKSPACE": str(workspace),
                    "GPU_PACKAGE_DIR": str(root / "packages"),
                    "ANTFRASTRUCTURE_DIR": str(root / "poison"),
                    "OBSERVED_HUB": str(observed),
                }
                result = subprocess.run(
                    ["/bin/bash", "-c", command],
                    env=env,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(
                    observed.read_text().strip(),
                    str(workspace / "third_party/ANTfrastructure"),
                )


if __name__ == "__main__":
    unittest.main()
