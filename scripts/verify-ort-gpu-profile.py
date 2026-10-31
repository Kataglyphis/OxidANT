#!/usr/bin/env python3
"""Require a profiled ONNX model operator to execute on the requested provider."""

import argparse
import collections
import json
import sys
from pathlib import Path

PROVIDERS = {
    "cpu": "CPUExecutionProvider",
    "cuda": "CUDAExecutionProvider",
    "migraphx": "MIGraphXExecutionProvider",
    "directml": "DmlExecutionProvider",
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("profile", type=Path)
    parser.add_argument("--device", choices=PROVIDERS, required=True)
    args = parser.parse_args()
    try:
        events = json.loads(args.profile.read_text(encoding="utf-8"))
        if not isinstance(events, list):
            raise ValueError("profile must be a list of events")
        counts = collections.Counter(
            event["args"]["provider"]
            for event in events
            if isinstance(event, dict)
            and event.get("cat") == "Node"
            and isinstance(event.get("args"), dict)
            and isinstance(event["args"].get("provider"), str)
        )
    except (OSError, ValueError, UnicodeError) as error:
        print(f"Invalid ORT profile: {error}", file=sys.stderr)
        return 1
    provider = PROVIDERS[args.device]
    if counts[provider] == 0:
        print(f"No model operators executed on {provider}: {dict(counts)}", file=sys.stderr)
        return 1
    print(f"Profiled operators: {dict(counts)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
