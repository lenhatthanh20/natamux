#!/usr/bin/env python3
"""Checks that Cargo.lock holds exactly the Zed crates of scripts/zed-lock-crates.txt.

Usage: zed-lock-check.py [<Cargo.lock> <crate list>]

A Zed crate is a package whose source is the Zed fork. cargo deny may skip them (they are
publish = false), so this list is the guard that no Zed crate comes back. Exit 0 when the two
sets are equal, 1 when not.
"""

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ZED_SOURCE = "git+https://github.com/lenhatthanh20/zed?"


def zed_crates(lock_text):
    lock = tomllib.loads(lock_text)
    return {p["name"] for p in lock.get("package", []) if p.get("source", "").startswith(ZED_SOURCE)}


def listed(list_text):
    lines = (line.strip() for line in list_text.splitlines())
    return {line for line in lines if line and not line.startswith("#")}


def problems(lock_text, list_text):
    found, allowed = zed_crates(lock_text), listed(list_text)
    return [f"not in the list: {name}" for name in sorted(found - allowed)] + [
        f"listed but not in Cargo.lock: {name}" for name in sorted(allowed - found)
    ]


def main(argv):
    if len(argv) not in (0, 2):
        print(__doc__, file=sys.stderr)
        return 2
    lock, crates = map(Path, argv or [ROOT / "Cargo.lock", ROOT / "scripts/zed-lock-crates.txt"])
    lock_text = lock.read_text()
    found = problems(lock_text, crates.read_text())
    for problem in found:
        print(f"::error::Zed crates: {problem}")
    if found:
        return 1
    print(f"Cargo.lock holds the {len(zed_crates(lock_text))} listed Zed crates.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
