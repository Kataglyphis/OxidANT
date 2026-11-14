"""JUnit from a `cargo test` log, so the quality report counts the tests and a flaky one has a record per run."""
# Stable libtest prints no per-test time, so only each binary's total is kept; a binary that died mid-run is one failure.
import argparse
import re
import sys
import xml.etree.ElementTree as ET

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
RESULT = re.compile(r"^test (?P<name>.+?) \.\.\. (?P<outcome>ok|FAILED|ignored)(?:, (?P<reason>.*))?$")
RUNNING = re.compile(r"^running (\d+) tests?$")
SUMMARY = re.compile(r"^test result: .*finished in (?P<seconds>[0-9.]+)s$")
# `Running unittests src/lib.rs (target/debug/deps/kataglyphis_core-<hash>)`, or under --verbose the binary's command line.
BINARY = re.compile(r"/deps/(?P<binary>[A-Za-z0-9_]+)-[0-9a-f]{16}\b")
UNITTESTS = re.compile(r"^Running (?P<what>unittests \S+|tests\S*|benches\S*|\S+\.rs) \(")
PACKAGE = re.compile(r"\bCARGO_PKG_NAME=(?P<package>\S+)")
DOCTESTS = re.compile(r"^Doc-tests (?P<package>\S+)$")
FAILURE_HEAD = re.compile(r"^---- (?P<name>.+) stdout ----$")
CRASH = re.compile(r"process didn't exit successfully: .*")


class Suite:
    """One test binary's results, in the order libtest printed them."""

    def __init__(self, name: str, planned: int) -> None:
        self.name = name
        self.planned = planned
        self.cases: list[tuple[str, str, str]] = []
        self.output: dict[str, list[str]] = {}
        self.seconds = 0.0
        self.finished = False
        self.crash = ""


def suite_label(line: str, current: str) -> str:
    """The name the next suite takes from a cargo status line, or the current one when the line names none."""
    doc = DOCTESTS.match(line)
    if doc:
        return f"{doc['package']}::doc-tests"
    if not line.startswith("Running ") or "--crate-name" in line or "rustdoc" in line:
        return current
    binary = BINARY.search(line)
    if binary is None:
        return current
    package = PACKAGE.search(line)
    what = UNITTESTS.match(line)
    if package:
        return f"{package['package']}::{binary['binary']}"
    if what:
        return f"{binary['binary']} ({what['what']})"
    return binary["binary"]


def parse(lines: list[str]) -> list[Suite]:
    """Every suite libtest started, with its cases and the captured output of each failure."""
    suites: list[Suite] = []
    label, failing = "", None
    for raw in lines:
        line = ANSI.sub("", raw).rstrip("\n").strip()
        current = suites[-1] if suites else None
        started = RUNNING.match(line)
        if started:
            suites.append(Suite(label or f"suite-{len(suites) + 1}", int(started.group(1))))
            failing = None
            continue
        label = suite_label(line, label)
        if current is None or current.finished:
            continue
        result = RESULT.match(line)
        if result:
            outcome = {"ok": "passed", "FAILED": "failed", "ignored": "skipped"}[result["outcome"]]
            current.cases.append((result["name"], outcome, result["reason"] or ""))
            continue
        head = FAILURE_HEAD.match(line)
        if head:
            failing = head["name"]
            current.output[failing] = []
            continue
        if line == "failures:":
            failing = None
            continue
        summary = SUMMARY.match(line)
        if summary:
            current.seconds = float(summary["seconds"])
            current.finished = True
            continue
        crash = CRASH.search(line)
        if crash:
            current.crash = crash.group(0)
            continue
        if failing is not None:
            current.output[failing].append(raw.rstrip("\n"))
    return suites


def to_xml(suites: list[Suite]) -> ET.Element:
    """A <testsuites> tree; a binary that died before its summary adds one failed case that says so."""
    root = ET.Element("testsuites", name="cargo test")
    totals = {"tests": 0, "failures": 0, "skipped": 0}
    for suite in suites:
        cases = list(suite.cases)
        if not suite.finished:
            ended = suite.crash or "the log ends before the binary's summary"
            done = len(cases)
            cases.append(("(binary ended early)", "failed", f"{done} of {suite.planned} tests reported; {ended}"))
        failures = sum(1 for _name, outcome, _note in cases if outcome == "failed")
        skipped = sum(1 for _name, outcome, _note in cases if outcome == "skipped")
        node = ET.SubElement(
            root, "testsuite", name=suite.name, tests=str(len(cases)),
            failures=str(failures), skipped=str(skipped), time=f"{suite.seconds:.3f}",
        )
        for name, outcome, note in cases:
            case = ET.SubElement(node, "testcase", classname=suite.name, name=name)
            if outcome == "skipped":
                ET.SubElement(case, "skipped", message=note)
            elif outcome == "failed":
                text = "\n".join(suite.output.get(name, [])).strip() or note
                failure = ET.SubElement(case, "failure", message=(text.splitlines() or [""])[0][:200])
                failure.text = text
        totals["tests"] += len(cases)
        totals["failures"] += failures
        totals["skipped"] += skipped
    for key, value in totals.items():
        root.set(key, str(value))
    return root


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", help="the cargo test output, stdout and stderr together")
    parser.add_argument("output", help="the JUnit file to write")
    args = parser.parse_args(argv)
    with open(args.log, encoding="utf-8", errors="replace") as handle:
        suites = parse(handle.readlines())
    if not suites:
        print(f"cargo-test-junit: no test binary ran in {args.log}", file=sys.stderr)
        return 1
    root = to_xml(suites)
    tree = ET.ElementTree(root)
    ET.indent(tree)
    tree.write(args.output, encoding="utf-8", xml_declaration=True)
    print(
        f"cargo-test-junit: {root.get('tests')} tests, {root.get('failures')} failed, "
        f"{root.get('skipped')} skipped in {len(suites)} binaries -> {args.output}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
