#!/usr/bin/env python3
"""Check real-image parity in three isolated processes from an extracted ZIP."""

from __future__ import annotations

import argparse
import hashlib
import itertools
import os
import re
import runpy
import subprocess
import sys
import tempfile
from pathlib import Path

FIXTURE_SHA256 = "6d375ac20fadfd400b3033084f8b0581dcc145f7c14e3b24b97975e379c52999"


def locked_files(fetcher: Path, lock: Path) -> list[str]:
    """Every file name the lock pins, through the hub's parser."""
    return [name for _, name, _ in runpy.run_path(str(fetcher))["read_lock"](lock)]


def model_check_arguments(
    locked: Path, stage: Path, lock: Path, fetcher: Path
) -> list[str]:
    """Check every file the lock pins against the extracted package."""
    command = [
        str(locked),
        "--root",
        str(stage),
        "--directory",
        str(stage / "resources/models"),
        "--lock",
        str(lock),
        "--fetcher",
        str(fetcher),
    ]
    for name in locked_files(fetcher, lock):
        command += ["--file", name]
    return command


def run(
    command: list[str], cwd: Path | None = None, env: dict[str, str] | None = None
) -> None:
    """Fail immediately when an executable or verifier fails."""
    subprocess.run(command, check=True, cwd=cwd, env=env)


def detector_command(
    executable: Path, fixture: Path, output: Path, profile: Path, repeat: Path | None
) -> list[str]:
    """Use the same packaged executable invoked by the candidate launcher."""
    command = [
        str(executable),
        "onnx-detect",
        "--image",
        str(fixture),
        "--output",
        str(output),
        "--profile",
        str(profile),
    ]
    return command + (["--repeat-output", str(repeat)] if repeat is not None else [])


def child_environment(stage: Path, device: str, index: str) -> dict[str, str]:
    """Select the bundled runtime and default detector settings for one process."""
    env = os.environ.copy()
    for key in list(env):
        name = key.upper()
        if name.startswith(("KATAGLYPHIS_", "ORT_", "MIGRAPHX_", "GST_")):
            del env[key]
    env["KATAGLYPHIS_ORT_DEVICE"] = device
    env["ORT_DYLIB_PATH"] = str(stage / "onnxruntime.dll")
    if device == "directml":
        env["KATAGLYPHIS_DIRECTML_DEVICE_ID"] = index
    if device == "cpu" and os.name == "nt":
        env["PATH"] = os.pathsep.join(
            (str(stage), str(Path(env["SYSTEMROOT"]) / "System32"))
        )
    return env


def check(stage: Path, device: str, index: str, repo: Path, temp_root: Path) -> Path:
    """Verify the locked ZIP payload, provider profiles and detector parity."""
    if (
        device not in {"cuda", "directml"}
        or not re.fullmatch(r"0|[1-9][0-9]*", index)
        or int(index) > 2147483647
    ):
        raise ValueError("invalid GPU provider or DirectML adapter index")
    stage = stage.resolve(strict=True)
    repo = repo.resolve(strict=True)
    hub = repo / "third_party" / "ANTfrastructure"
    fixture = repo / "crates/inference/tests/fixtures/two_cats.jpg"
    locked = hub / "shared/python/verify_locked_files.py"
    comparator = hub / "shared/python/compare_detector_parity.py"
    intervals = hub / "shared/python/verify_ort_profile.py"
    fetcher = hub / "linux/scripts/lib/fetch-data-lock.py"
    lock = repo / "resources/models/models.lock"
    dxgi = hub / "windows/scripts/diagnostics/verify-dxgi-adapter.py"
    executable = stage / "kataglyphis_cli.exe"
    runtime = stage / "onnxruntime.dll"
    for path in (
        fixture,
        locked,
        comparator,
        intervals,
        fetcher,
        lock,
        executable,
        runtime,
    ):
        if not path.is_file():
            raise ValueError(f"required packaged file or pinned helper missing: {path}")
    if hashlib.sha256(fixture.read_bytes()).hexdigest() != FIXTURE_SHA256:
        raise ValueError("two-cats fixture does not match its expected digest")
    py = [sys.executable, "-I"]
    run(py + model_check_arguments(locked, stage, lock, fetcher))
    if device == "directml":
        if not dxgi.is_file():
            raise ValueError("pinned physical DXGI adapter verifier missing")
        run(py + [str(dxgi), "--vendor", "amd", "--index", index])
    work = Path(tempfile.mkdtemp(prefix="oxidant-windows-parity-", dir=temp_root))
    for label, selected in (("cpu", "cpu"), ("gpu1", device), ("gpu2", device)):
        output = work / f"{label}.json"
        profile = work / f"{label}-profile"
        repeat = work / f"{label}-repeat.json"
        run(
            detector_command(executable, fixture, output, profile, repeat),
            cwd=stage,
            env=child_environment(stage, selected, index),
        )
        profiles = list(work.glob(f"{label}-profile*.json"))
        if len(profiles) != 1 or not output.is_file() or not repeat.is_file():
            raise ValueError(
                f"expected one {label} detector output and provider profile"
            )
        run(
            py
            + [
                str(intervals),
                str(profiles[0]),
                "--device",
                selected,
                "--min-runs",
                "2",
            ]
        )
    labels = ("cpu", "cpu-repeat", "gpu1", "gpu1-repeat", "gpu2", "gpu2-repeat")
    for left, right in itertools.combinations(labels, 2):
        mode = [] if left == "cpu" else ["--pairwise"]
        run(
            py
            + [str(comparator), str(work / f"{left}.json"), str(work / f"{right}.json")]
            + mode
        )
    return work


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("stage", type=Path)
    parser.add_argument("--device", choices=("cuda", "directml"), required=True)
    parser.add_argument("--directml-index", default="0")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    try:
        work = check(
            args.stage,
            args.device,
            args.directml_index,
            repo,
            Path(os.environ["RUNNER_TEMP"]),
        )
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Windows detector parity failed: {error}\n")
    print(f"Windows ZIP detector parity passed; profiles: {work}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
