#!/usr/bin/env python3
"""Fail on a Slang `import` whose module's top-level names the importing file never mentions."""
import argparse
import re
import sys
from pathlib import Path

COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)
IMPORT = re.compile(r"^\s*(__exported\s+)?import\s+(\w+)\s*;", re.M)
DEFINE = re.compile(r"^\s*#\s*define\s+(\w+)", re.M)
DIRECTIVE = re.compile(r"^\s*#[^\n]*", re.M)
IDENT = re.compile(r"[A-Za-z_]\w*")
# The declared name is the last identifier before the first of these at the statement's top level.
NAME_END = re.compile(r"[(=;:{\[]")


def top_level_statements(text):
    """Yield each depth-0 statement's head, the text before its first brace or semicolon."""
    depth, head = 0, []
    for char in text:
        if depth == 0 and char in ";{":
            yield "".join(head)
            head = []
        if char == "{":
            depth += 1
        elif char == "}":
            depth = max(depth - 1, 0)
        elif depth == 0 and char != ";":
            head.append(char)


def strip_attributes(head):
    """Drop leading `[attr(...)]` groups, which a declaration may carry before its type."""
    head = head.strip()
    while head.startswith("["):
        depth = 0
        for index, char in enumerate(head):
            depth += {"[": 1, "]": -1}.get(char, 0)
            if depth == 0:
                head = head[index + 1:].strip()
                break
        else:
            return ""
    return head


def declared_names(text):
    names = set(DEFINE.findall(text))
    for head in top_level_statements(DIRECTIVE.sub(" ", text)):
        head = strip_attributes(head)
        if not head or head.startswith(("import", "__exported")):
            continue
        cut = NAME_END.search(head)
        idents = IDENT.findall(head[: cut.start()] if cut else head)
        if idents:
            names.add(idents[-1])
    return names


def load_modules(root):
    modules = {}
    for path in sorted(root.rglob("*.slang")):
        text = COMMENT.sub(" ", path.read_text(encoding="utf-8"))
        modules[path] = text
    return modules


def exports_by_stem(modules):
    """Each module's own names plus those of the modules it re-exports with `__exported import`."""
    own = {path.stem: declared_names(text) for path, text in modules.items()}
    reexports = {path.stem: [m[1] for m in IMPORT.findall(text) if m[0]] for path, text in modules.items()}
    resolved = {}

    def names(stem, seen=()):
        if stem not in resolved:
            found = set(own.get(stem, ()))
            for child in reexports.get(stem, ()):
                if child not in seen:
                    found |= names(child, seen + (stem,))
            resolved[stem] = found
        return resolved[stem]

    return {stem: names(stem) for stem in own}


def dead_imports(root):
    modules = load_modules(root)
    exports = exports_by_stem(modules)
    dead = []
    for path, text in modules.items():
        used = set(IDENT.findall(IMPORT.sub(" ", text)))
        for exported, module in IMPORT.findall(text):
            if module not in exports:
                dead.append(f"{path.relative_to(root)}: import {module} resolves to no .slang file under {root}")
            elif not exported and not exports[module] & used:
                dead.append(f"{path.relative_to(root)}: import {module} is unused; no name it declares appears here")
    return dead


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, nargs="?", default=Path(__file__).resolve().parents[1] / "shaders")
    root = parser.parse_args().root.resolve()
    dead = dead_imports(root)
    if dead:
        sys.exit("Drop the dead Slang import(s):\n  " + "\n  ".join(dead))
    print(f"Every Slang import under {root} is used")


if __name__ == "__main__":
    main()
