#!/usr/bin/env python3
"""Exercise the GPU proof against real ORT event shapes and misleading profiles."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[2] / "verify-ort-gpu-profile.py"


class ProfileTest(unittest.TestCase):
    def run_profile(self, profile, device):
        with tempfile.TemporaryDirectory() as work:
            path = Path(work) / "profile.json"
            path.write_text(json.dumps(profile), encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(SCRIPT), str(path), "--device", device],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_real_node_event_requires_the_requested_provider(self):
        profile = [
            {
                "cat": "Node",
                "args": {"provider": "CPUExecutionProvider", "op_name": "Conv"},
            },
            {
                "cat": "Node",
                "args": {
                    "provider": "MIGraphXExecutionProvider",
                    "op_name": "FusedGraph",
                },
            },
        ]
        self.assertEqual(self.run_profile(profile, "cpu").returncode, 0)
        self.assertEqual(self.run_profile(profile, "migraphx").returncode, 0)
        self.assertNotEqual(self.run_profile(profile, "cuda").returncode, 0)
        self.assertNotEqual(self.run_profile(profile, "directml").returncode, 0)

    def test_registration_event_alone_does_not_prove_inference(self):
        profile = [{"cat": "Session", "args": {"provider": "CUDAExecutionProvider"}}]
        self.assertNotEqual(self.run_profile(profile, "cuda").returncode, 0)

    def test_all_supported_gpu_providers_accept_fused_computation(self):
        for device, provider in (
            ("cuda", "CUDAExecutionProvider"),
            ("migraphx", "MIGraphXExecutionProvider"),
            ("directml", "DmlExecutionProvider"),
        ):
            with self.subTest(device=device):
                profile = [
                    {
                        "cat": "Node",
                        "args": {"provider": provider, "op_name": "FusedGraph"},
                    }
                ]
                self.assertEqual(self.run_profile(profile, device).returncode, 0)
                profile[0]["args"]["op_name"] = " MemcpyFromHost "
                self.assertNotEqual(self.run_profile(profile, device).returncode, 0)

    def test_transfers_and_unidentified_nodes_are_not_gpu_computation(self):
        profile = [
            {
                "cat": "Node",
                "args": {
                    "provider": "CUDAExecutionProvider",
                    "op_name": "MemcpyFromHost",
                },
            },
            {
                "cat": "Node",
                "args": {
                    "provider": "CUDAExecutionProvider",
                    "op_name": "MemcpyToHost",
                },
            },
            {"cat": "Node", "args": {"provider": "MIGraphXExecutionProvider"}},
            {
                "cat": "Node",
                "args": {"provider": "CPUExecutionProvider", "op_name": "Conv"},
            },
        ]
        self.assertEqual(self.run_profile(profile, "cpu").returncode, 0)
        self.assertNotEqual(self.run_profile(profile, "cuda").returncode, 0)
        self.assertNotEqual(self.run_profile(profile, "migraphx").returncode, 0)

    def test_wrong_top_level_and_empty_nodes_fail(self):
        self.assertNotEqual(self.run_profile({}, "cuda").returncode, 0)
        self.assertNotEqual(
            self.run_profile([{"cat": "Node", "args": None}], "cuda").returncode, 0
        )


if __name__ == "__main__":
    unittest.main()
