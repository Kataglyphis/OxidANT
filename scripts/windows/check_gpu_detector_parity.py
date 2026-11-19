#!/usr/bin/env python3
"""Check real-image parity in three isolated processes from an extracted ZIP; the run is the hub's."""

from __future__ import annotations

import argparse
import importlib
import os
import runpy
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "third_party/ANTfrastructure/shared/python"))
# ty resolves no import through the path above, so a static import fails the strict Python lane.
shared = importlib.import_module("check_detector_parity")


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


def check(stage: Path, device: str, index: str, repo: Path, temp_root: Path) -> Path:
    """Verify the locked ZIP payload, then the hub's provider profiles and detector parity."""
    shared.validate_device(device, index)
    stage = stage.resolve(strict=True)
    repo = repo.resolve(strict=True)
    hub = repo / "third_party" / "ANTfrastructure"
    fixture = repo / "crates/inference/tests/fixtures/two_cats.jpg"
    locked = hub / "shared/python/verify_locked_files.py"
    fetcher = hub / "linux/scripts/lib/fetch-data-lock.py"
    lock = repo / "resources/models/models.lock"
    executable = stage / "kataglyphis_cli.exe"
    for path in (fixture, locked, fetcher, lock, executable):
        if not path.is_file():
            raise ValueError(f"required packaged file or pinned helper missing: {path}")
    shared.verify_runtime(stage)
    shared.verify_fixture(fixture)
    run([sys.executable, "-I"] + model_check_arguments(locked, stage, lock, fetcher))
    shared.verify_adapter(hub, device, index, run)
    work = shared.new_work("oxidant-windows-parity-", temp_root)
    return shared.check_parity(
        hub,
        stage,
        device,
        index,
        work,
        lambda _label, output, repeat, profile: detector_command(
            executable, fixture, output, profile, repeat
        ),
        lambda *a, **k: run(*a, **k),
    )


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
