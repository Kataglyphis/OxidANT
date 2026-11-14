"""The C declarations of every GenAI symbol crates/text/src/genai.rs binds, against a committed snapshot of ort_genai_c.h."""
# genai.rs types the function pointers by hand, so a GenAI bump that changes a signature still compiles; this fails it.
import argparse
import difflib
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
BINDINGS = ROOT / "crates/text/src/genai.rs"
SNAPSHOT = ROOT / "crates/text/genai-signatures.txt"
HEADER = pathlib.Path("/usr/local/lib/onnxruntime-genai/include/ort_genai_c.h")

BOUND = re.compile(r'(?:bind|lookup)!\(\s*library\s*,\s*"(Oga\w+)"\s*\)')
DECLARED = re.compile(r"OGA_EXPORT\s+(.+?)\s+OGA_API_CALL\s+(Oga\w+)\s*\((.*?)\)\s*;", re.DOTALL)
COMMENT = re.compile(r"/\*.*?\*/|//[^\n]*", re.DOTALL)


def bound_symbols(bindings: pathlib.Path) -> list[str]:
    """Every symbol genai.rs binds by name, the optional lookups included, sorted."""
    return sorted(set(BOUND.findall(bindings.read_text(encoding="utf-8"))))


def declarations(header: pathlib.Path) -> dict[str, str]:
    """Each exported function of the header as one line: name, return type and parameters, whitespace collapsed."""
    text = COMMENT.sub(" ", header.read_text(encoding="utf-8"))
    found = {}
    for returns, name, params in DECLARED.findall(text):
        found[name] = f"{name}: {' '.join(returns.split())} ({' '.join(params.split())})"
    return found


def current_lines(bindings: pathlib.Path, header: pathlib.Path) -> list[str]:
    """The snapshot as this header gives it; a bound symbol the header lacks is a line that says so."""
    declared = declarations(header)
    return [declared.get(name, f"{name}: NOT DECLARED in {header.name}") for name in bound_symbols(bindings)]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--header", type=pathlib.Path, default=HEADER, help="GenAI's ort_genai_c.h")
    parser.add_argument("--bindings", type=pathlib.Path, default=BINDINGS)
    parser.add_argument("--snapshot", type=pathlib.Path, default=SNAPSHOT)
    parser.add_argument("--write", action="store_true", help="write the snapshot from the header instead of checking")
    args = parser.parse_args(argv)
    if not args.header.is_file():
        print(f"check-genai-signatures: no GenAI header at {args.header}", file=sys.stderr)
        return 2
    lines = current_lines(args.bindings, args.header)
    if not lines:
        print(f"check-genai-signatures: no bind! or lookup! of an Oga symbol in {args.bindings}", file=sys.stderr)
        return 2
    if args.write:
        args.snapshot.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"check-genai-signatures: wrote {len(lines)} signatures to {args.snapshot}")
        return 0
    committed = args.snapshot.read_text(encoding="utf-8").splitlines() if args.snapshot.is_file() else []
    if committed == lines:
        print(f"check-genai-signatures: OK, {len(lines)} bound GenAI signatures match {args.snapshot.name}")
        return 0
    print(
        f"check-genai-signatures: the bound GenAI signatures differ from {args.snapshot}; check genai.rs's "
        "function types against the header, then rerun with --write:",
        file=sys.stderr,
    )
    sys.stderr.writelines(
        f"{line}\n" for line in difflib.unified_diff(committed, lines, "snapshot", "header", lineterm="")
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
