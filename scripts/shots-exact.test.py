#!/usr/bin/env python3
"""Tests of shots-exact.py. Run: python3 scripts/shots-exact.test.py"""

import contextlib
import importlib.util
import io
import pathlib
import struct
import tempfile
import unittest
import zlib

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("shots_exact", HERE / "shots-exact.py")
shots_exact = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shots_exact)

W, H = 20, 10


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if pa <= pb and pa <= pc else b if pb <= pc else c


def rgba_png(rows, width, filters):
    """An RGBA PNG; row y uses filter filters[y % len(filters)], as screencapture may."""
    stride, raw, prev = width * 4, b"", bytes(width * 4)
    for y, row in enumerate(rows):
        kind = filters[y % len(filters)]
        out = bytearray(stride)
        for i in range(stride):
            a = row[i - 4] if i >= 4 else 0
            b = prev[i]
            c = prev[i - 4] if i >= 4 else 0
            guess = [0, a, b, (a + b) >> 1, paeth(a, b, c)][kind]
            out[i] = (row[i] - guess) & 255
        raw += bytes([kind]) + bytes(out)
        prev = row

    def chunk(kind, body):
        crc = struct.pack(">I", zlib.crc32(kind + body))
        return struct.pack(">I", len(body)) + kind + body + crc

    head = struct.pack(">IIBBBBB", width, len(rows), 8, 6, 0, 0, 0)
    return (
        shots_exact.PNG_SIGNATURE
        + chunk(b"IHDR", head)
        + chunk(b"iCCP", b"profile\x00\x00" + zlib.compress(b"x"))
        + chunk(b"IDAT", zlib.compress(raw)[:7])
        + chunk(b"IDAT", zlib.compress(raw)[7:])
        + chunk(b"IEND", b"")
    )


def pixels(width=W, height=H, change=None):
    """RGBA rows with a gradient; `change` maps (x, y) to a new RGB."""
    rows = []
    for y in range(height):
        row = bytearray()
        for x in range(width):
            rgb = (change or {}).get((x, y), ((x * 13) & 255, (y * 29) & 255, (x * y) & 255))
            row += bytes(rgb) + b"\xff"
        rows.append(bytes(row))
    return rows


def png(width=W, height=H, change=None, filters=(0,)):
    return rgba_png(pixels(width, height, change), width, filters)


class DecodeTest(unittest.TestCase):
    def test_every_filter_decodes_to_the_same_pixels(self):
        plain = shots_exact.decode_png(png())
        for kind in range(5):
            self.assertEqual(shots_exact.decode_png(png(filters=(kind,))), plain, f"filter {kind}")
        self.assertEqual(shots_exact.decode_png(png(filters=(4, 1, 3, 2, 0))), plain)

    def test_alpha_is_ignored(self):
        rows = pixels()
        clear = [bytes(b if i % 4 != 3 else 0 for i, b in enumerate(row)) for row in rows]
        self.assertEqual(
            shots_exact.decode_png(rgba_png(rows, W, (0,))),
            shots_exact.decode_png(rgba_png(clear, W, (0,))),
        )

    def test_reads_its_own_rgb_png(self):
        _, _, rows = shots_exact.decode_png(png())
        self.assertEqual(shots_exact.decode_png(shots_exact.encode_png(W, rows)), (W, H, rows))

    def test_refuses_a_file_that_is_not_a_png(self):
        with self.assertRaises(ValueError):
            shots_exact.decode_png(b"BM......")


class CompareTest(unittest.TestCase):
    def decoded(self, **kwargs):
        return shots_exact.decode_png(png(**kwargs))

    def test_equal_images_pass(self):
        self.assertEqual(shots_exact.compare(self.decoded(), self.decoded(filters=(4,)))[0], None)

    def test_one_changed_pixel_fails(self):
        problem, bad, _ = shots_exact.compare(self.decoded(), self.decoded(change={(3, 4): (1, 2, 3)}))
        self.assertEqual((problem, bad), ("differs", {(3, 4)}))

    def test_a_size_change_fails(self):
        problem, _, _ = shots_exact.compare(self.decoded(), self.decoded(width=W + 1))
        self.assertEqual(problem, f"size {W + 1}x{H}, base {W}x{H}")

    def test_the_control_mask_allows_a_moving_pixel_only(self):
        base = self.decoded()
        control = self.decoded(change={(3, 4): (9, 9, 9)})
        moved = self.decoded(change={(3, 4): (7, 7, 7)})
        self.assertEqual(shots_exact.compare(base, moved, control)[:2], (None, set()))
        both = self.decoded(change={(3, 4): (7, 7, 7), (5, 5): (7, 7, 7)})
        self.assertEqual(shots_exact.compare(base, both, control)[:2], ("differs", {(5, 5)}))

    def test_boxes_join_near_pixels_only(self):
        found = shots_exact.boxes({(0, 0), (5, 2), (40, 40), (41, 48)})
        self.assertEqual(found, [(0, 0, 5, 2), (40, 40, 41, 48)])
        # (8, 5) grows the first box until it reaches the box of (16, 0).
        self.assertEqual(shots_exact.boxes({(0, 0), (16, 0), (8, 5)}), [(0, 0, 16, 5)])


class MainTest(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.folder.name)
        self.base, self.new, self.control = root / "base", root / "new", root / "control"
        for folder in (self.base, self.new, self.control):
            folder.mkdir()
        self.out = root / "diffs"

    def tearDown(self):
        self.folder.cleanup()

    def put(self, folder, name, data):
        (folder / f"{name}.png").write_bytes(data)

    def run_main(self, *extra):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            code = shots_exact.main([str(self.base), str(self.new), *extra])
        return code, output.getvalue()

    def test_equal_folders_pass(self):
        self.put(self.base, "tasks-dark-800x560", png())
        self.put(self.new, "tasks-dark-800x560", png(filters=(2,)))
        self.assertEqual(self.run_main()[0], 0)

    def test_a_shot_with_no_base_fails(self):
        self.put(self.new, "tasks-dark-800x560", png())
        code, output = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("NO BASE  tasks-dark-800x560", output)

    def test_a_base_shot_with_no_new_shot_fails(self):
        for name in ("a", "b", "window-dark-800x560"):
            self.put(self.base, name, png())
        self.put(self.new, "a", png())
        code, output = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("NO NEW   b", output)
        self.assertNotIn("window", output)
        self.assertEqual(output.splitlines()[-1], "2 shots, 1 failed")
        self.assertEqual(self.run_main("--only", "a")[0], 0)

    def test_a_state_named_diff_is_compared(self):
        self.put(self.base, "diff-x", png())
        self.put(self.new, "diff-x", png(change={(1, 1): (0, 0, 0)}))
        code, output = self.run_main()
        self.assertEqual(code, 1)
        self.assertIn("DIFFERS  diff-x: 1 px, boxes (1,1)-(1,1)", output)

    def test_window_shots_are_skipped(self):
        self.put(self.new, "window-dark-800x560", png())
        self.put(self.base, "tasks", png())
        self.put(self.new, "tasks", png())
        code, output = self.run_main()
        self.assertEqual((code, output.splitlines()[-1]), (0, "1 shots, 0 failed"))

    def test_no_shot_compared_fails(self):
        self.put(self.base, "tasks", png())
        self.put(self.new, "tasks", png())
        self.assertEqual(self.run_main("--only", "nothing")[0], 1)
        self.assertEqual(self.run_main("--only", "nothing", "task")[0], 0)

    def test_the_control_mask_passes_a_moving_pixel(self):
        self.put(self.base, "tasks", png())
        self.put(self.control, "tasks", png(change={(2, 2): (1, 1, 1)}))
        self.put(self.new, "tasks", png(change={(2, 2): (5, 5, 5)}))
        self.assertEqual(self.run_main()[0], 1)
        code, output = self.run_main("--control", str(self.control))
        self.assertEqual(code, 0)
        self.assertIn("(1 moving px)", output)

    def test_the_diff_image_marks_the_differing_pixels_red(self):
        self.put(self.base, "tasks", png())
        self.put(self.new, "tasks", png(change={(2, 3): (0, 0, 0)}))
        self.assertEqual(self.run_main("--out", str(self.out))[0], 1)
        _, _, rows = shots_exact.read_png(self.out / "diff-tasks.png")
        self.assertEqual(rows[3][6:9], b"\xff\x00\x00")
        self.assertNotEqual(rows[3][3:6], b"\xff\x00\x00")

    def test_out_inside_an_input_folder_is_refused(self):
        self.put(self.base, "tasks", png())
        self.put(self.new, "tasks", png())
        for folder in (self.base, self.new, self.control):
            code, output = self.run_main("--control", str(self.control), "--out", str(folder / "d"))
            self.assertEqual(code, 2)
            self.assertIn("is inside an input folder", output)
            self.assertFalse((folder / "d").exists())


if __name__ == "__main__":
    unittest.main()
