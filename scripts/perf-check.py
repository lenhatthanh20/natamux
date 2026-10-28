#!/usr/bin/env python3
"""Checks the perf numbers of a perf.yml run against their targets and the baseline (docs/perf.md).

Usage: perf-check.py [--baseline FILE] [--medians FILE] OUTPUT...

OUTPUT    text files with the numbers, one run per line. A line is a JSON object, or
          `<name>: {json}` (--bench-flows, daemon_bench), or `<name>: key=value ...` (--smoke,
          motion-cost.sh). Other lines are skipped, so a whole log can be passed.
--baseline  the baseline numbers (default scripts/perf-baseline.json); null: no baseline yet.
--medians   also write the medians as JSON, to fill the baseline from a run.

Each number is the median of its runs. It fails when a gated number is over its target, when a
number is more than 20% worse than its baseline, or when a gated number is missing. Lower is
better unless the name is in HIGHER_IS_BETTER. Prints a markdown table for the job summary.
"""

import argparse
import json
import pathlib
import statistics
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORSE_SHARE = 0.20
# One refresh of a 60 Hz display, as the CI runner's.
REFRESH_MS = 16.7

# name: (target, gated). Not gated: shown, never fails, as no CI run meets it yet (docs/perf.md).
# Targets: T-0221 report "Targets" and docs/plan-rust-migration.md.
TARGETS = {
    "first_frame_ms": (300, True),
    "first_data_ms": (1000, True),
    "task_switch_seen_p95_ms": (REFRESH_MS, False),
    "task_switch_unseen_p95_ms": (300, True),
    "event_to_row_p95_ms": (50, True),
    "event_ui_p95_ms": (1, False),
    "diff_click_p95_ms": (100, True),
    "palette_key_p95_ms": (REFRESH_MS, False),
    "resize_frame_p99_ms": (REFRESH_MS, False),
    "board_frame_p95_ms": (REFRESH_MS, False),
    "app_rss_mb": (800, True),
    "daemon_busy_rss_mb": (1024, True),
    "task_init_2000_ms": (300, True),
    "hook_p95_ms": (5, False),
    "echo_flood_p95_ms": (10, True),
    "echo_agents_p95_ms": (5, True),
    # The report's 0.5% and 1% are not met on CI: the gate is the T-0223 "after" + 20%.
    "table_background_cpu_pct": (1.0, True),
    "table_focus_cpu_pct": (2.5, True),
    "table_background_draws": (0.2, True),
}
HIGHER_IS_BETTER = set()


def parse_line(line):
    """The numbers of one line, or {} when it holds none."""
    text = line.strip()
    if text.startswith("{"):
        body = text
    else:
        _, colon, body = text.partition(": ")
        if not colon:
            return {}
        body = body.strip()
    if body.startswith("{"):
        try:
            data = json.loads(body)
        except ValueError:
            return {}
        if not isinstance(data, dict):
            return {}
        return {
            key: float(value)
            for key, value in data.items()
            if isinstance(value, (int, float)) and not isinstance(value, bool)
        }
    numbers = {}
    for token in body.split():
        key, equals, value = token.partition("=")
        if not equals:
            return {}
        try:
            numbers[key] = float(value)
        except ValueError:
            # `first_data_ms=none`: not measured in this run.
            pass
    return numbers


def collect(texts):
    """Every run of every number, from the texts of the output files."""
    runs = {}
    for text in texts:
        for line in text.splitlines():
            for key, value in parse_line(line).items():
                runs.setdefault(key, []).append(value)
    return runs


def worse_than(value, baseline, higher):
    if higher:
        return value < baseline * (1 - WORSE_SHARE)
    return value > baseline * (1 + WORSE_SHARE)


def over(value, target, higher):
    return value < target if higher else value > target


def check(runs, baseline):
    """Rows of (name, median, runs, target, baseline, result, failed), sorted by name."""
    rows = []
    for name in sorted(set(runs) | set(TARGETS) | set(baseline)):
        values = runs.get(name, [])
        target, gated = TARGETS.get(name, (None, False))
        base = baseline.get(name)
        higher = name in HIGHER_IS_BETTER
        if not values:
            failed = gated
            result = "missing" if gated else "no data"
            rows.append((name, None, 0, target, base, result, failed))
            continue
        median = statistics.median(values)
        problems = []
        failed = False
        if target is not None and over(median, target, higher):
            problems.append("over target" if gated else "over target (not gated yet)")
            failed = gated
        if base is not None and worse_than(median, base, higher):
            problems.append(f"{int(WORSE_SHARE * 100)}% worse than baseline")
            failed = True
        result = ", ".join(problems) or "ok"
        rows.append((name, median, len(values), target, base, result, failed))
    return rows


def number(value):
    if value is None:
        return "-"
    return f"{value:.2f}".rstrip("0").rstrip(".")


def table(rows):
    lines = [
        "| measure | median | runs | target | baseline | result |",
        "|---|---|---|---|---|---|",
    ]
    for name, median, count, target, base, result, failed in rows:
        mark = "**FAIL** " if failed else ""
        lines.append(
            f"| {name} | {number(median)} | {count} | {number(target)} | {number(base)} "
            f"| {mark}{result} |"
        )
    return "\n".join(lines)


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--baseline", default=str(ROOT / "scripts" / "perf-baseline.json"))
    parser.add_argument("--medians")
    parser.add_argument("outputs", nargs="+")
    args = parser.parse_args(argv)
    baseline = json.loads(pathlib.Path(args.baseline).read_text())
    texts = [pathlib.Path(path).read_text(errors="replace") for path in args.outputs]
    runs = collect(texts)
    rows = check(runs, baseline)
    print("### Perf check\n")
    print(table(rows))
    if args.medians:
        medians = {name: statistics.median(values) for name, values in sorted(runs.items())}
        pathlib.Path(args.medians).write_text(json.dumps(medians, indent=2) + "\n")
    failed = [row[0] for row in rows if row[6]]
    if failed:
        print(f"\nperf-check: failed: {', '.join(failed)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
