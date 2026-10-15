"""Line coverage per crate from tarpaulin's cobertura.xml, each against its own floor, so one well-tested crate cannot hide another."""
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

    covered, total = measure(args.report)
    failed = []
    for name in sorted(set(total) | set(floors)):
        if total[name] == 0:
            percent = 0.0
        else:
            percent = 100.0 * covered[name] / total[name]
        floor = floors.get(name)
        verdict = "-" if floor is None else ("ok" if percent >= floor else "BELOW")
        print(f"{name:28} {covered[name]:6} / {total[name]:6}  {percent:5.1f} %  floor {'-' if floor is None else f'{floor:g} %':>6}  {verdict}")
        if floor is not None and percent < floor:
            failed.append(name)
    if failed:
        print(f"Coverage below its floor: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
