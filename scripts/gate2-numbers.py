#!/usr/bin/env python3
"""Prints one `gate2: gate2_...=...` line for scripts/perf-check.py from a `--gate2` run.

Usage: gate2-numbers.py OUTPUT

OUTPUT    the app's output; its `gate2: <part> report <path>` lines name the perf reports.
Fails when no report gave a number.
"""

import json
import pathlib
import sys

# part: [(name, path in the report)]
NUMBERS = {
    "panes": [("gate2_panes_frame_p99_ms", ("summary", "p99_ms"))],
    "hidden": [
        ("gate2_hidden_draws_per_s", ("hidden_phase", "draws_per_second")),
        ("gate2_hidden_cpu_pct", ("hidden_phase", "cpu_percent")),
    ],
    "show-latency": [("gate2_show_latency_p95_ms", ("show_latency", "p95_ms"))],
    "typing": [
        ("gate2_key_to_glyph_p95_ms", ("summary", "key_to_glyph_p95_ms")),
        ("gate2_key_to_echo_p95_ms", ("summary", "key_to_echo_p95_ms")),
    ],
}


def reports(text):
    """{part: path} of the `gate2: <part> report <path>` lines."""
    found = {}
    for line in text.splitlines():
        words = line.strip().split(" ", 3)
        if len(words) == 4 and words[0] == "gate2:" and words[2] == "report":
            found[words[1]] = words[3]
    return found


def pick(report, path):
    value = report
    for key in path:
        if not isinstance(value, dict):
            return None
        value = value.get(key)
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return value
    return None


def numbers(text, read=lambda path: json.loads(pathlib.Path(path).read_text())):
    found = {}
    for part, path in reports(text).items():
        report = read(path)
        for name, keys in NUMBERS.get(part, []):
            value = pick(report, keys)
            if value is not None:
                found[name] = value
    return found


def line(found):
    return "gate2: " + " ".join(f"{name}={value:.2f}" for name, value in sorted(found.items()))


def main(argv):
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    found = numbers(pathlib.Path(argv[0]).read_text(errors="replace"))
    if not found:
        print("gate2-numbers: no report gave a number", file=sys.stderr)
        return 1
    print(line(found))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
