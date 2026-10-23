#!/usr/bin/env python3
"""Compares the terminal bench of the Rust daemon with the Node one, run in the same CI job
(docs/rust-phase4.md, "Bench"). Each file is the output of one bench; its TERMINAL_BENCH_JSON line
holds the results. Writes one table (to GITHUB_STEP_SUMMARY too) and fails when a gated number of
Rust is worse than Node's.

Usage: compare-terminal-bench.py <node output> <rust output>
"""

import json
import os
import sys

# (measure, field, better): the numbers the phase-4 gate holds Rust to.
GATES = [
    ("snapshot", "serializeMs", "lower"),
    ("snapshot", "attachMs", "lower"),
    ("parse", "mbPerSecond", "higher"),
]
SHOWN = ["serializeMs", "attachMs", "maxLoopDelayMs", "mbPerSecond", "frames", "bytesPerFrame", "framesPerSecond"]


def results(text):
    for line in text.splitlines():
        at = line.find("TERMINAL_BENCH_JSON ")
        if at >= 0:
            return json.loads(line[at + len("TERMINAL_BENCH_JSON "):])
    raise ValueError("no TERMINAL_BENCH_JSON line")


def key(row):
    return (row["measure"], row.get("cols"), row.get("style"))


def compare(node, rust):
    """The table rows and the failed gates."""
    by_key = {key(row): row for row in rust}
    rows, failed = [], []
    for left in node:
        right = by_key.get(key(left))
        if right is None:
            failed.append(f"{key(left)}: no Rust result")
            continue
        name = " ".join(str(part) for part in key(left) if part is not None)
        for field in SHOWN:
            if field not in left or field not in right:
                continue
            gate = next((better for measure, gated, better in GATES if measure == left["measure"] and gated == field), None)
            ok = gate is None or (right[field] <= left[field] if gate == "lower" else right[field] >= left[field])
            verdict = "-" if gate is None else ("pass" if ok else "FAIL")
            rows.append(f"| {name} | {field} | {left[field]} | {right[field]} | {verdict} |")
            if not ok:
                failed.append(f"{name} {field}: node {left[field]}, rust {right[field]}")
    return rows, failed


def main(argv):
    with open(argv[1]) as node_file, open(argv[2]) as rust_file:
        node, rust = results(node_file.read()), results(rust_file.read())
    rows, failed = compare(node, rust)
    table = "\n".join(["| case | measure | node | rust | gate |", "|---|---|---|---|---|", *rows])
    print(table)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as out:
            out.write(f"\n### Terminal bench, Node and Rust daemons\n\n{table}\n")
    for line in failed:
        print(f"worse than Node: {line}", file=sys.stderr)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
