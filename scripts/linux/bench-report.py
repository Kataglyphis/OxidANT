"""Criterion's results under a target dir as a Markdown table, with a warning per benchmark slower than the threshold."""
import json
import pathlib
import sys


def main():
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "target/criterion")
    threshold = float(sys.argv[2]) if len(sys.argv) > 2 else 0.25
    rows = []
    for new in sorted(root.glob("**/new/estimates.json")):
        bench = new.parent.parent
        mean_ns = json.loads(new.read_text())["mean"]["point_estimate"]
        change_file = bench / "change" / "estimates.json"
        change = json.loads(change_file.read_text())["mean"]["point_estimate"] if change_file.is_file() else None
        rows.append((str(bench.relative_to(root)), mean_ns, change))
    if not rows:
        print("No benchmark results under", root)
        return 0
    print("| Benchmark | Mean | Change against the cached baseline |")
    print("| --- | --- | --- |")
    for name, mean_ns, change in rows:
        mean = f"{mean_ns / 1e6:.2f} ms" if mean_ns >= 1e5 else f"{mean_ns / 1e3:.1f} µs"
        delta = "no baseline yet" if change is None else f"{change * 100:+.1f} %"
        print(f"| `{name}` | {mean} | {delta} |")
        # A runner is noisy, so a slowdown warns rather than fails.
        if change is not None and change > threshold:
            print(f"::warning title=Benchmark slower::{name} {change * 100:+.1f} % against the cached baseline", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
