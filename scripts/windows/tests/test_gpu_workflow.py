"""The Windows GPU candidate selects only the requested image/provider pairs."""

import re
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


class GpuWorkflow(unittest.TestCase):
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
