#!/usr/bin/env python3
"""Tests of gate2-numbers.py. Run: python3 -I scripts/gate2-numbers.test.py"""

import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    "gate2_numbers", pathlib.Path(__file__).with_name("gate2-numbers.py")
)
gate2 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate2)

OUTPUT = """gate2: 30 terminals printing agent (6 shown)
gate2: bench on T-0001 for 60 s
gate2: bench report /logs/perf-1.json
gate2: panes report /logs/perf-3.json
gate2: hidden report /logs/perf-4.json
gate2: show-latency report /logs/perf-5.json
gate2: typing 100 keys into Terminal 990 of T-0001
gate2: typing report /logs/perf-6.json
"""

REPORTS = {
    "/logs/perf-1.json": {"summary": {"p99_ms": 99.0}},
    "/logs/perf-3.json": {"summary": {"p99_ms": 33.4}},
    "/logs/perf-4.json": {"hidden_phase": {"draws_per_second": 0.1, "cpu_percent": None}},
    "/logs/perf-5.json": {"show_latency": {"p95_ms": 80.0}},
    "/logs/perf-6.json": {"summary": {"key_to_glyph_p95_ms": 18.5, "key_to_echo_p95_ms": 2.25}},
}


class Numbers(unittest.TestCase):
    def test_reads_each_part_and_skips_a_missing_number(self):
        found = gate2.numbers(OUTPUT, read=REPORTS.__getitem__)
        self.assertEqual(
            found,
            {
                "gate2_panes_frame_p99_ms": 33.4,
                "gate2_hidden_draws_per_s": 0.1,
                "gate2_show_latency_p95_ms": 80.0,
                "gate2_key_to_glyph_p95_ms": 18.5,
                "gate2_key_to_echo_p95_ms": 2.25,
            },
        )

    def test_the_line_is_one_perf_check_line(self):
        line = gate2.line({"gate2_b": 2.25, "gate2_a": 18.5})
        self.assertEqual(line, "gate2: gate2_a=18.50 gate2_b=2.25")

    def test_no_report_gives_nothing(self):
        self.assertEqual(gate2.numbers("gate2: bench on T-0001 for 60 s\n"), {})


if __name__ == "__main__":
    unittest.main()
