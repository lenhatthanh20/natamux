#!/usr/bin/env python3
"""Tests of compare-shots.py. Run: python3 scripts/compare-shots.test.py"""

import importlib.util
import os
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).parent


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, HERE / file)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


compare_shots = load("compare_shots", "compare-shots.py")
# Reuses the BMP writer of the sibling test.
bmp = load("check_shots_test", "check-shots.test.py").bmp

W, H = 100, 50


def flat(colour):
    return lambda x, y: colour


def with_patch(colour, patch, count):
    """A flat shot where the first `count` pixels of row 0 have the colour `patch`."""
    return lambda x, y: patch if y == 0 and x < count else colour


class CompareShotsTest(unittest.TestCase):
    def test_equal_shots_match(self):
        shot = bmp(W, H, flat((40, 44, 52)))
        problems, diff, share = compare_shots.compare(shot, shot)
        self.assertEqual((problems, share), ([], 0.0))
        self.assertIsNotNone(diff)

    def test_a_small_channel_change_is_ignored(self):
        shot = bmp(W, H, flat((40, 44, 52)))
        near = bmp(W, H, flat((56, 60, 68)), bits=32)
        self.assertEqual(compare_shots.compare(near, shot)[0], [])

    def test_a_change_over_the_channel_limit_counts(self):
        shot = bmp(W, H, flat((40, 44, 52)))
        far = bmp(W, H, flat((57, 44, 52)))
        problems, _, share = compare_shots.compare(far, shot)
        self.assertEqual(share, 1.0)
        self.assertEqual(len(problems), 1)

    def test_half_a_percent_passes_and_a_bit_more_fails(self):
        base = bmp(W, H, flat((0, 0, 0)))
        # 5000 pixels: 25 is 0.5%.
        ok = bmp(W, H, with_patch((0, 0, 0), (255, 255, 255), 25))
        bad = bmp(W, H, with_patch((0, 0, 0), (255, 255, 255), 26))
        self.assertEqual(compare_shots.compare(ok, base)[0], [])
        self.assertEqual(len(compare_shots.compare(bad, base)[0]), 1)

    def test_fails_when_the_sizes_differ(self):
        problems, diff, _ = compare_shots.compare(
            bmp(W, H, flat((0, 0, 0))), bmp(W + 1, H, flat((0, 0, 0)))
        )
        self.assertIn("100x50 but the baseline is 101x50", problems[0])
        self.assertIsNone(diff)

    def test_the_diff_image_marks_differing_pixels_red(self):
        base = bmp(W, H, flat((0, 0, 0)))
        shot = bmp(W, H, with_patch((0, 0, 0), (255, 255, 255), 3))
        _, diff, _ = compare_shots.compare(shot, base)
        _, _, rows = compare_shots.read_bmp(diff)
        # The test BMP is bottom-up, so its row 0 is the last row here.
        self.assertEqual(rows[-1][0:3], b"\xff\x00\x00")
        self.assertNotEqual(rows[-1][9:12], b"\xff\x00\x00")
        self.assertNotEqual(rows[-2][0:3], b"\xff\x00\x00")

    def test_refuses_a_file_that_is_not_a_bmp(self):
        with self.assertRaises(ValueError):
            compare_shots.compare(b"\x89PNG....", b"\x89PNG....")

    def test_report_only_exits_0_and_writes_the_diff(self):
        with tempfile.TemporaryDirectory() as folder:
            paths = [os.path.join(folder, name) for name in ("shot.bmp", "base.bmp", "diff.bmp")]
            pathlib.Path(paths[0]).write_bytes(bmp(W, H, flat((255, 255, 255))))
            pathlib.Path(paths[1]).write_bytes(bmp(W, H, flat((0, 0, 0))))
            self.assertEqual(compare_shots.main(paths), 1)
            os.remove(paths[2])
            self.assertEqual(compare_shots.main(["--report-only", *paths]), 0)
            self.assertTrue(os.path.exists(paths[2]))


if __name__ == "__main__":
    unittest.main()
