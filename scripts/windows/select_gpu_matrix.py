#!/usr/bin/env python3
"""Select the supported Windows GPU candidate and hardware-profile rows."""

import json
import os
from pathlib import Path

ROWS = (
    {
        "vendor": "nvidia",
        "tag": "winamd64-nvidia",
        "provider": "cuda",
        "runner": "gpu-nvidia-cuda",
    },
    {
        "vendor": "amd",
        "tag": "winamd64-rocm",
        "provider": "directml",
        "runner": "gpu-amd-directml",
    },
)


def main() -> None:
    vendor = os.environ.get("GPU_VENDOR", "all")
    if vendor not in {"all", "nvidia", "amd"}:
        raise ValueError("gpu-vendor must be all, nvidia or amd")
    rows = [row for row in ROWS if vendor == "all" or row["vendor"] == vendor]
    output = json.dumps({"include": rows}, separators=(",", ":"))
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as stream:
        stream.write(f"matrix={output}\n")


if __name__ == "__main__":
    main()
