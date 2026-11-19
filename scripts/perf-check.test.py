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

    def test_a_reported_number_shows_and_never_fails(self):
        rows = perf.check({"gated_ms": [1], "shown_ms": [1]}, {})
        self.assertEqual(failed(rows), [])
        self.assertEqual(result(rows, "term_draws_per_wake"), "no data")
        rows = perf.check({"gated_ms": [1], "shown_ms": [1], "term_draws_per_wake": [900]}, {})
        self.assertEqual(failed(rows), [])
        self.assertEqual(result(rows, "term_draws_per_wake"), "ok")

    def test_the_daemon_bench_rows_are_reported_and_a_slower_parse_is_worse(self):
        line = 'terminal_bench: {"parse_sgr_mb_s": 80.5, "flood_frames_per_s": 60, "snapshot_200_sgr_bytes": 9}'
        numbers = perf.parse_line(line)
        self.assertEqual(
            numbers, {"parse_sgr_mb_s": 80.5, "flood_frames_per_s": 60.0, "snapshot_200_sgr_bytes": 9.0}
        )
        staged = {"hook_staged_single_p50_ms", "hook_staged_paced_p95_ms", "hook_staged_paced_max_ms"}
        shells = {
            "daemon_idle_30_shells_rss_mb",
            "daemon_idle_shell_kib",
            "daemon_busy_grown_mb",
            "daemon_busy_shell_mb",
        }
        self.assertTrue(set(numbers) | staged | shells <= perf.REPORTED)
        self.assertNotIn("flood_frames_per_s", perf.HIGHER_IS_BETTER)
        rows = perf.check({"gated_ms": [1], "shown_ms": [1]}, {})
        self.assertEqual(failed(rows), [])
        self.assertEqual(result(rows, "tui_wire_over_raw"), "no data")
        rows = perf.check({"gated_ms": [1], "shown_ms": [1], "parse_sgr_mb_s": [70]}, {"parse_sgr_mb_s": 100})
        self.assertEqual(failed(rows), ["parse_sgr_mb_s"])
        rows = perf.check({"gated_ms": [1], "shown_ms": [1], "parse_sgr_mb_s": [130]}, {"parse_sgr_mb_s": 100})
        self.assertEqual(failed(rows), [])

    def test_the_app_rows_are_reported_and_read_from_their_lines(self):
        lines = [
            "motion_cost: task_focus_cpu_pct=2.4 task_background_cpu_pct=0.6 "
            "task_focus_draws=2.1 task_background_draws=0.1",
            "binary_size: app_binary_mb=61.25 daemon_binary_mb=20.5 cli_binary_mb=9.75",
            "gate2: gate2_key_to_glyph_p95_ms=18.50 gate2_hidden_draws_per_s=0.10",
            'flow_bench: {"terminal_key_flood_to_glyph_p95_ms": 24.1, '
            '"app_rss_per_hidden_terminal_mb": 9.5}',
            "smoke: first_frame_ms=120.0 first_data_ms=200.0 keymap_ms=1.5 first_draw_ms=40.2 "
            "app_rss_smoke_mb=150.3",
        ]
        runs = perf.collect(["\n".join(lines)])
        self.assertTrue(set(runs) - {"first_frame_ms", "first_data_ms"} <= perf.REPORTED)
        self.assertEqual(runs["app_binary_mb"], [61.25])
        self.assertEqual(runs["gate2_key_to_glyph_p95_ms"], [18.5])
        rows = perf.check({"gated_ms": [1], "shown_ms": [1], **runs}, {})
        self.assertEqual(failed(rows), [])
        self.assertEqual(result(rows, "terminal_key_to_echo_p95_ms"), "no data")
        self.assertEqual(result(rows, "task_focus_draws"), "ok")

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
