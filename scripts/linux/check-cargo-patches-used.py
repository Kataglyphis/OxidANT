#!/usr/bin/env python3
"""Fail when a [patch.crates-io] source copy is missing from Cargo.lock, which cargo only warns about."""

import argparse
import sys
import tomllib
from pathlib import Path


def load(path: Path) -> dict:
    with path.open("rb") as source:
        return tomllib.load(source)


def patched_packages(root: Path) -> list[tuple[str, str, str]]:
    """(name, version, path) of every path entry under [patch.crates-io]."""
    patches = load(root / "Cargo.toml").get("patch", {}).get("crates-io", {})
    found = []
    for spec in patches.values():
        if not isinstance(spec, dict) or "path" not in spec:
            continue
        package = load(root / spec["path"] / "Cargo.toml")["package"]
        found.append((package["name"], package["version"], spec["path"]))
    return found


def locked_path_packages(lock: Path) -> set[tuple[str, str]]:
    """Path packages carry no `source` in Cargo.lock."""
    packages = load(lock)["package"]
    if not isinstance(packages, list) or not packages:
        raise ValueError("Cargo.lock has no packages")
    return {(p["name"], p["version"]) for p in packages if "source" not in p}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "root", type=Path, help="workspace root holding Cargo.toml and Cargo.lock"
    )
    args = parser.parse_args()
    try:
        patches = patched_packages(args.root)
        locked = locked_path_packages(args.root / "Cargo.lock")
    except (OSError, KeyError, TypeError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"cargo patches: invalid manifest or lockfile: {error}", file=sys.stderr)
        return 2
    unused = [p for p in patches if (p[0], p[1]) not in locked]
    for name, version, path in unused:
        print(
            f"cargo patches: {path} ({name} {version}) is not in Cargo.lock; "
            f"pin the dependency that pulls {name} to the patched version",
            file=sys.stderr,
        )
    if unused:
        return 1
    print(f"cargo patches: all {len(patches)} patched source copies are in Cargo.lock")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
