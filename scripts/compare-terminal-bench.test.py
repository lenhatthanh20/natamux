#!/usr/bin/env python3
"""Tests of compare-terminal-bench.py. Run: python3 scripts/compare-terminal-bench.test.py"""

import importlib.util
import pathlib
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("bench", HERE / "compare-terminal-bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

NODE = [
    {"measure": "snapshot", "cols": 80, "style": "plain", "serializeMs": 40, "attachMs": 90, "frames": 9},
    {"measure": "parse", "style": "sgr", "mbPerSecond": 30},
]


class Compare(unittest.TestCase):
    def test_reads_the_json_line_of_a_bench_output(self):
        text = "noise\nstdout | TERMINAL_BENCH_JSON [{\"measure\":\"parse\"}]\nmore"
        self.assertEqual(bench.results(text), [{"measure": "parse"}])

    def test_passes_when_rust_is_at_least_as_good(self):
        rust = [dict(NODE[0], serializeMs=5, attachMs=6, frames=30), dict(NODE[1], mbPerSecond=90)]
        rows, failed = bench.compare(NODE, rust)
        self.assertEqual(failed, [])
        self.assertIn("| snapshot 80 plain | frames | 9 | 30 | - |", rows)

    def test_fails_a_gated_number_that_is_worse_or_missing(self):
        rust = [dict(NODE[0], attachMs=91)]
        _, failed = bench.compare(NODE, rust)
        self.assertEqual(len(failed), 2)
        self.assertIn("attachMs", failed[0])
        self.assertIn("no Rust result", failed[1])


class Medians(unittest.TestCase):
    def runs(self, serialize):
        return [[dict(NODE[0], serializeMs=ms), NODE[1]] for ms in serialize]

    def test_one_slow_node_sample_does_not_decide(self):
        node = bench.medians(self.runs([40, 80, 40, 41, 39]))
        self.assertEqual(node[0]["serializeMs"], 40)
        rust = [dict(NODE[0], serializeMs=40), NODE[1]]
        _, failed = bench.compare(node, rust)
        self.assertEqual(failed, [])

    def test_medians_that_fail(self):
        node = bench.medians(self.runs([40, 41, 39, 40, 42]))
        rust = bench.medians([[dict(NODE[0], serializeMs=ms), NODE[1]] for ms in [60, 61, 10, 62, 63]])
        _, failed = bench.compare(node, rust)
        self.assertEqual(len(failed), 1)
        self.assertIn("serializeMs", failed[0])

    def test_one_run_per_side_is_unchanged(self):
        self.assertEqual(bench.medians([NODE]), NODE)

    def test_sides_read_both_command_line_forms(self):
        self.assertEqual(bench.sides(["x", "n", "r"]), (["n"], ["r"]))
        self.assertEqual(bench.sides(["x", "--node", "n1", "n2", "--rust", "r1"]), (["n1", "n2"], ["r1"]))


if __name__ == "__main__":
    unittest.main()
