import importlib.util
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("native_record", Path(__file__).with_name("native-record.py"))
CAPTURE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAPTURE)


class CutValidationTests(unittest.TestCase):
    def test_hidpi_dell_uses_mode_pixels_without_guessing_from_scale(self):
        display = {"index": 3, "name": "DELL P3223QE", "scale": 2,
                   "logicalWidth": 1920, "logicalHeight": 1080,
                   "pixelWidth": 3840, "pixelHeight": 2160}
        status = {"screenRecordingAllowed": True, "displays": [display]}
        self.assertEqual(CAPTURE.select_display(status, 3), display)
        display.update(pixelWidth=1920, pixelHeight=1080)
        with self.assertRaises(ValueError):
            CAPTURE.select_display(status, 3)

    def manifest(self, **overrides):
        clip = {"name": "review-dark", "start": 1, "end": 11, "crop": [0, 0, 3840, 2160]}
        clip.update(overrides)
        return {"version": 1, "clips": [clip]}

    def test_original_must_be_true_4k(self):
        for width, height in [(1920, 1080), (3840, 2158), (7680, 4320)]:
            with self.subTest(width=width, height=height), self.assertRaises(ValueError):
                CAPTURE.require_source({"width": width, "height": height, "duration": 20})
        CAPTURE.require_source({"width": 3840, "height": 2160, "duration": 20})

    def test_crop_and_cuts_cannot_escape_the_original(self):
        for crop in [[-2, 0, 3840, 2160], [2, 0, 3840, 2160], [0, 0, 3840, 2162]]:
            with self.subTest(crop=crop), self.assertRaises(ValueError):
                CAPTURE.validate_clips(self.manifest(crop=crop), 20)
        for start, end in [(-1, 10), (1, 21), (10, 1), (1, float("nan"))]:
            with self.subTest(start=start, end=end), self.assertRaises(ValueError):
                CAPTURE.validate_clips(self.manifest(start=start, end=end), 20)

    def test_output_names_cannot_escape_destination(self):
        for name in ["../../source", "/source", "review-dark.mp4", ""]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                CAPTURE.validate_clips(self.manifest(name=name), 20)

    def test_an_explicit_crop_keeps_its_aspect_without_upscaling(self):
        manifest = self.manifest(crop=[192, 80, 3456, 2000], width=1920)
        self.assertEqual(CAPTURE.validate_clips(manifest, 20), manifest["clips"])
        with self.assertRaises(ValueError):
            CAPTURE.validate_clips(self.manifest(crop=[0, 0, 1600, 1000], width=1920), 20)

    def test_duplicate_names_cannot_replace_a_previous_clip(self):
        manifest = self.manifest()
        manifest["clips"].append(dict(manifest["clips"][0]))
        with self.assertRaises(ValueError):
            CAPTURE.validate_clips(manifest, 20)

    def test_montage_preserves_source_order_and_uses_a_visible_poster(self):
        manifest = self.manifest(ranges=[[2, 5], [12, 18]], posterAt=14)
        self.assertEqual(CAPTURE.validate_clips(manifest, 20), manifest["clips"])
        for ranges in [[], [[12, 18], [2, 5]], [[2, 5], [4, 8]], [[2, 21]]]:
            with self.subTest(ranges=ranges), self.assertRaises(ValueError):
                CAPTURE.validate_clips(self.manifest(ranges=ranges), 20)
        with self.assertRaises(ValueError):
            CAPTURE.validate_clips(self.manifest(ranges=[[2, 5], [12, 18]], posterAt=9), 20)


if __name__ == "__main__":
    unittest.main()
