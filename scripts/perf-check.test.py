#!/usr/bin/env python3
"""Tests of perf-check.py. Run: python3 scripts/perf-check.test.py"""

import contextlib
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "perf_check", pathlib.Path(__file__).with_name("perf-check.py")
)
perf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(perf)


def failed(rows):
    return [row[0] for row in rows if row[6]]


def result(rows, name):
    return next(row[5] for row in rows if row[0] == name)


class Lines(unittest.TestCase):
    def test_reads_every_output_kind(self):
        self.assertEqual(perf.parse_line('{"a": 1, "b": 2.5}'), {"a": 1.0, "b": 2.5})
        self.assertEqual(
            perf.parse_line('flow_bench: {"diff_click_p95_ms": 12.5, "tasks": 200}'),
            {"diff_click_p95_ms": 12.5, "tasks": 200.0},
        )
        self.assertEqual(
            perf.parse_line("smoke: first_frame_ms=120.5 first_data_ms=none fonts_ms=3.0"),
            {"first_frame_ms": 120.5, "fonts_ms": 3.0},
        )

    def test_skips_lines_without_numbers(self):
        for line in [
            "",
            "smoke: ok",
            'smoke: typed "abc" into the text box',
            "flow_bench: report /tmp/perf-1.json",
            "draws 3 active true",
            "test result: ok. 1 passed; 0 failed",
            "daemon_bench: {not json",
            '{"flag": true, "name": "x"}',
        ]:
            self.assertEqual(perf.parse_line(line), {}, line)

    def test_collects_the_runs_of_each_number(self):
        runs = perf.collect(['flow_bench: {"x": 1}\nflow_bench: {"x": 3}\n', "smoke: x=2"])
        self.assertEqual(runs, {"x": [1.0, 3.0, 2.0]})


class Check(unittest.TestCase):
    def setUp(self):
        self.targets = perf.TARGETS
        perf.TARGETS = {"gated_ms": (100, True), "shown_ms": (10, False)}

    def tearDown(self):
        perf.TARGETS = self.targets

    def test_within_target_and_baseline_passes(self):
        rows = perf.check({"gated_ms": [50, 90, 200], "shown_ms": [5]}, {"gated_ms": 80})
        self.assertEqual(failed(rows), [])
        self.assertEqual(result(rows, "gated_ms"), "ok")

    def test_over_a_gated_target_fails(self):
        rows = perf.check({"gated_ms": [150, 120, 90], "shown_ms": [5]}, {})
        self.assertEqual(failed(rows), ["gated_ms"])
        self.assertEqual(result(rows, "gated_ms"), "over target")

    def test_over_a_target_that_is_not_gated_only_shows(self):
        rows = perf.check({"gated_ms": [1], "shown_ms": [20, 30, 40]}, {})
        self.assertEqual(failed(rows), [])
        self.assertEqual(result(rows, "shown_ms"), "over target (not gated yet)")

    def test_more_than_20_percent_worse_than_the_baseline_fails(self):
        rows = perf.check({"gated_ms": [61], "shown_ms": [5], "other": [13]}, {"gated_ms": 50, "other": 10})
        self.assertEqual(failed(rows), ["gated_ms", "other"])
        rows = perf.check({"gated_ms": [60], "shown_ms": [5], "other": [12]}, {"gated_ms": 50, "other": 10})
        self.assertEqual(failed(rows), [])

    def test_a_null_baseline_checks_nothing(self):
        rows = perf.check({"gated_ms": [99], "shown_ms": [9]}, {"gated_ms": None})
        self.assertEqual(failed(rows), [])

    def test_higher_is_better_turns_the_checks_around(self):
        perf.HIGHER_IS_BETTER.add("gated_ms")
        try:
            rows = perf.check({"gated_ms": [79], "shown_ms": [1]}, {"gated_ms": 120})
            self.assertEqual(result(rows, "gated_ms"), "over target, 20% worse than baseline")
            rows = perf.check({"gated_ms": [150], "shown_ms": [1]}, {"gated_ms": 120})
            self.assertEqual(failed(rows), [])
        finally:
            perf.HIGHER_IS_BETTER.discard("gated_ms")

    def test_a_missing_gated_number_fails(self):
        rows = perf.check({"shown_ms": [1]}, {})
        self.assertEqual(failed(rows), ["gated_ms"])
        self.assertEqual(result(rows, "gated_ms"), "missing")


class Main(unittest.TestCase):
    def test_prints_a_table_writes_medians_and_returns_the_result(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = pathlib.Path(folder)
            baseline = folder / "baseline.json"
            baseline.write_text(json.dumps({"hook_p95_ms": None}))
            output = folder / "out.txt"
            output.write_text('daemon_bench: {"hook_p95_ms": 2, "echo_flood_p95_ms": 3}\n')
            medians = folder / "medians.json"
            text = io.StringIO()
            with contextlib.redirect_stdout(text), contextlib.redirect_stderr(io.StringIO()):
                code = perf.main(["--baseline", str(baseline), "--medians", str(medians), str(output)])
            # echo_agents_p95_ms is gated and missing.
            self.assertEqual(code, 1)
            self.assertIn("| hook_p95_ms | 2 | 1 | 5 | - | ok |", text.getvalue())
            self.assertIn("| echo_agents_p95_ms | - | 0 | 5 | - | **FAIL** missing |", text.getvalue())
            self.assertEqual(json.loads(medians.read_text())["echo_flood_p95_ms"], 3)

    def test_the_baseline_file_names_only_known_numbers(self):
        baseline = json.loads(pathlib.Path(perf.ROOT / "scripts" / "perf-baseline.json").read_text())
        self.assertTrue(set(perf.TARGETS) <= set(baseline))


if __name__ == "__main__":
    unittest.main()
