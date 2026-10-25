#!/usr/bin/env python3
"""Tests of check-shots.py. Run: python3 scripts/check-shots.test.py"""

import importlib.util
import pathlib
import struct
import unittest

spec = importlib.util.spec_from_file_location(
    "check_shots", pathlib.Path(__file__).with_name("check-shots.py")
)
check_shots = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_shots)


def bmp(width, height, colour_at, bits=24):
    """A bottom-up BMP whose pixel (x, y) has colour_at(x, y)."""
    step = bits // 8
    row = (width * step + 3) // 4 * 4
    body = bytearray()
    for y in range(height):
        line = bytearray()
        for x in range(width):
            r, g, b = colour_at(x, y)
            line += bytes([b, g, r] + ([255] if bits == 32 else []))
        body += line + bytes(row - len(line))
    header = struct.pack("<2sIHHI", b"BM", 54 + len(body), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, width, height, 1, bits, 0, len(body), 0, 0, 0, 0)
    return header + info + bytes(body)


def window(background, text):
    # A band of text colour across a background, like a window with some labels.
    return lambda x, y: text if 10 <= y < 20 else background


class CheckShotsTest(unittest.TestCase):
    def test_passes_a_dark_and_a_light_window(self):
        dark = bmp(64, 40, window((40, 44, 52), (220, 220, 220)))
        light = bmp(64, 40, window((250, 250, 250), (30, 30, 30)), bits=32)
        self.assertEqual(check_shots.check(dark, light), [])

    def test_fails_when_both_shots_look_the_same(self):
        same = bmp(64, 40, window((40, 44, 52), (220, 220, 220)))
        problems = check_shots.check(same, same)
        self.assertEqual(len(problems), 1)
        self.assertIn("not clearly brighter", problems[0])

    def test_fails_a_flat_shot(self):
        dark = bmp(64, 40, lambda x, y: (0, 0, 0))
        light = bmp(64, 40, window((250, 250, 250), (30, 30, 30)))
        self.assertEqual(check_shots.check(dark, light), ["the dark shot is one flat colour"])

    def test_refuses_a_file_that_is_not_a_bmp(self):
        with self.assertRaises(ValueError):
            check_shots.check(b"\x89PNG....", b"\x89PNG....")


if __name__ == "__main__":
    unittest.main()
