"""Criterion's means as a Markdown table against the median of earlier runs: warns when slower, fails when far slower."""
import argparse
import json
import pathlib
import statistics
import sys

# Fewer earlier runs than this only warn, so one odd runner cannot become the yardstick.
MIN_HISTORY_TO_FAIL = 3
# Below this a mean is cache and clock noise on a shared runner: it warns, never fails.
GATED_FROM_NS = 100_000


def current_means(root):
    """Benchmark name -> mean in ns, from each `<bench>/new/estimates.json` Criterion wrote."""
    means = {}
    for new in sorted(pathlib.Path(root).glob("**/new/estimates.json")):
        bench = new.parent.parent
        means[str(bench.relative_to(root))] = json.loads(new.read_text())["mean"]["point_estimate"]
    return means


def history(path):
    """Earlier runs' means: every *.json in a directory."""
    p = pathlib.Path(path)
    return [json.loads(f.read_text()) for f in sorted(p.glob("*.json"))] if p.is_dir() else []


def readable(ns):
    return f"{ns / 1e6:.2f} ms" if ns >= 1e5 else f"{ns / 1e3:.2f} µs"


def judge(name, mean_ns, earlier, args):
    """The row's change cell, and a failure message when the mean is far over the median of `earlier`."""
    if not earlier:
        return "no history yet", None
    median = statistics.median(earlier)
    change = mean_ns / median - 1
    cell = f"{change * 100:+.1f} % (median {readable(median)} of {len(earlier)})"
    if change > args.fail and len(earlier) >= MIN_HISTORY_TO_FAIL and median >= GATED_FROM_NS:
        print(f"::error title=Benchmark far slower::{name} {change * 100:+.1f} % against the median of {len(earlier)} runs", file=sys.stderr)
        return cell, f"{name} {change * 100:+.1f} %"
    if change > args.warn:
        print(f"::warning title=Benchmark slower::{name} {change * 100:+.1f} % against the median of {len(earlier)} runs", file=sys.stderr)
    return cell, None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("criterion", nargs="?", default="target/criterion")
    parser.add_argument("history", nargs="?", default=".bench-history", help="a directory of earlier runs' means")
    parser.add_argument("--warn", type=float, default=0.25, help="slower than the median by this fraction warns")
    parser.add_argument("--fail", type=float, default=1.0, help=f"slower by this fraction fails, for means from {GATED_FROM_NS} ns")
    parser.add_argument("--write", metavar="FILE", help="also write this run's means as JSON, for the history")
    args = parser.parse_args()

    means = current_means(args.criterion)
    if not means:
        print("No benchmark results under", args.criterion)
        return 0
    if args.write:
        pathlib.Path(args.write).write_text(json.dumps(means, indent=2, sort_keys=True))
    runs = history(args.history)
    print("| Benchmark | Mean | Change against earlier runs |")
    print("| --- | --- | --- |")
    failed = []
    for name, mean_ns in means.items():
        earlier = [r[name] for r in runs if isinstance(r.get(name), (int, float))]
        cell, failure = judge(name, mean_ns, earlier, args)
        print(f"| `{name}` | {readable(mean_ns)} | {cell} |")
        if failure:
            failed.append(failure)
    if failed:
        print(f"\nFar slower than before: {', '.join(failed)}.")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
