"""Line coverage per crate from tarpaulin's cobertura.xml, each against its own floor, so one well-tested crate cannot hide another."""
# Does not cover branch coverage, files outside crates/ and src/, or whether the tests behind a covered line assert anything.
import argparse
import collections
import sys
import xml.etree.ElementTree as ET


def area(filename):
    """crates/<name> for a workspace member's file, src for the root crate's, None for anything else."""
    parts = filename.replace("\\", "/").lstrip("./").split("/")
    if len(parts) >= 3 and parts[0] == "crates":
        return f"crates/{parts[1]}"
    if parts[0] == "src":
        return "src"
    return None


def measure(path):
    covered, total = collections.Counter(), collections.Counter()
    for cls in ET.parse(path).getroot().iter("class"):
        name = area(cls.get("filename", ""))
        if name is None:
            continue
        for line in cls.iter("line"):
            total[name] += 1
            if int(line.get("hits", "0")) > 0:
                covered[name] += 1
    return covered, total


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", help="cobertura.xml from cargo tarpaulin --out Xml")
    parser.add_argument("--floor", action="append", default=[], metavar="AREA=PERCENT",
                        help="e.g. crates/observant=80 or src=50; repeatable")
    parser.add_argument("--max-slack", type=float, default=None, metavar="POINTS",
                        help="warn, without failing, when a crate beats its floor by more than POINTS")
    args = parser.parse_args()

    floors = {}
    for spec in args.floor:
        name, _, value = spec.partition("=")
        try:
            floors[name] = float(value)
        except ValueError:
            parser.error(f"bad --floor {spec!r}: want AREA=PERCENT")
    if not floors:
        parser.error("no --floor given")

    return report(*measure(args.report), floors, args.max_slack)


def grade(percent, floor, max_slack):
    """(verdict, slack points worth a warning or None) for one crate."""
    if floor is None:
        return "NO FLOOR", None
    if percent < floor:
        return "BELOW", None
    over = percent - floor
    return "ok", over if max_slack is not None and over > max_slack else None


def report(covered, total, floors, max_slack):
    """Print the per-crate table and the slack warnings; 1 when a crate is under or lacks its floor."""
    bad = {"BELOW": [], "NO FLOOR": []}
    warnings = []
    for name in sorted(set(total) | set(floors)):
        percent = 100.0 * covered[name] / total[name] if total[name] else 0.0
        floor = floors.get(name)
        verdict, over = grade(percent, floor, max_slack)
        bad.get(verdict, []).append(name)
        if over is not None:
            warnings.append(f"{name} {percent:.1f} % is {over:.1f} points over its {floor:g} % floor")
        print(f"{name:28} {covered[name]:6} / {total[name]:6}  {percent:5.1f} %  floor {'-' if floor is None else f'{floor:g} %':>6}  {verdict}")
    for line in warnings:
        print(f"warning: floor slack over {max_slack:g} points: {line}; raise the floor")
    if bad["BELOW"]:
        print(f"Coverage below its floor: {', '.join(bad['BELOW'])}", file=sys.stderr)
    # A crate with no floor would let its coverage fall to zero unseen.
    if bad["NO FLOOR"]:
        print(f"Crate without a coverage floor: {', '.join(bad['NO FLOOR'])}", file=sys.stderr)
    return 1 if bad["BELOW"] or bad["NO FLOOR"] else 0


if __name__ == "__main__":
    sys.exit(main())
