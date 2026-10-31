#!/usr/bin/env python3
"""Compare a fresh compiler output tree with the renderer's committed WGSL."""
import argparse
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("generated", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    manifest = json.loads((root / "shaders/shader-manifest.json").read_text(encoding="utf-8"))
    changed = []
    for row in manifest["wgslMap"]:
        relative = Path(row["dst"]) / row["out"]
        generated = args.generated / relative
        committed = root / relative
        if not generated.is_file() or generated.read_text(encoding="utf-8") != committed.read_text(encoding="utf-8"):
            changed.append(str(relative))
    if changed:
        parser.exit(1, "Regenerate owned WGSL with compile-slang-shaders.sh or Build-SlangShaders.ps1:\n  " + "\n  ".join(changed) + "\n")
    print(f"All {len(manifest['wgslMap'])} generated WGSL files match their owned Slang source")


if __name__ == "__main__":
    main()
