#!/usr/bin/env python3
"""Keep obsolete Torch/Zstd packages out of Cargo.lock; Burn behavior needs Rust tests."""

import argparse
import re
import sys
import tomllib
from pathlib import Path


def version(value: str) -> tuple[int, int, int]:
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:[-+].*)?", value)
    if match is None:
        raise ValueError(f"invalid Cargo package version: {value!r}")
    return tuple(map(int, match.groups()))


def forbidden(name: str, value: str) -> bool:
    number = version(value)
    if name in {"burn-tch", "tch", "torch-sys"}:
        return True
    if name == "zip":
        return number[:2] == (0, 6)
    if name == "zstd":
        return number < (0, 12, 0)
    if name == "zstd-safe":
        return number < (8, 2, 0)
    if name == "zstd-sys":
        return number < (2, 1, 2)
    return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("lock", type=Path, help="Cargo.lock to check")
    args = parser.parse_args()
    try:
        with args.lock.open("rb") as source:
            packages = tomllib.load(source)["package"]
        if not isinstance(packages, list) or not packages:
            raise ValueError("Cargo.lock has no packages")
        findings = []
        for package in packages:
            name, value = package["name"], package["version"]
            if not isinstance(name, str) or not isinstance(value, str):
                raise ValueError("Cargo.lock has a package without a string name/version")
            if forbidden(name, value):
                findings.append(f"{name} {value}")
    except (OSError, KeyError, TypeError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"burn lock graph: invalid lockfile: {error}", file=sys.stderr)
        return 2
    if findings:
        for finding in findings:
            print(f"burn lock graph: forbidden dependency: {finding}", file=sys.stderr)
        return 1
    print("burn lock graph: old Torch/ZIP/Zstd chain absent")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
