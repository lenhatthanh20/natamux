#!/usr/bin/env python3
"""Regressions for native media metadata and missing theme assets."""

import importlib.util
from pathlib import Path
import tempfile
import unittest


SPEC = importlib.util.spec_from_file_location("native_media", Path(__file__).with_name("check-native-media.py"))
MEDIA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MEDIA)


class NativeMediaTests(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory()
        self.addCleanup(self.folder.cleanup)
        self.media = Path(self.folder.name)

    def manifest(self, names=("hero",)):
        return {
            "appVersion": "1.0.17",
            "capturedAt": "2026-11-03",
            "recordings": [{"name": name, "width": 1920, "height": 1080} for name in names],
        }

    def write_media(self, name="hero"):
        for theme in ("dark", "light"):
            (self.media / f"{name}-{theme}.mp4").write_bytes(b"\x00\x00\x00\x20ftypisom")
            (self.media / f"{name}-{theme}.webp").write_bytes(b"RIFF\x20\x00\x00\x00WEBP")

    def test_pending_is_valid_until_completeness_is_required(self):
        manifest = {"appVersion": None, "capturedAt": None, "recordings": []}
        self.assertEqual(MEDIA.validate_manifest(manifest, self.media), [])
        self.assertIn("still pending", MEDIA.validate_manifest(manifest, self.media, True)[0])

    def test_both_theme_pairs_are_required(self):
        self.write_media()
        self.assertEqual(MEDIA.validate_manifest(self.manifest(), self.media), [])
        (self.media / "hero-light.webp").rename(self.media / "unlisted.webp")
        self.assertIn("hero-light.webp", MEDIA.validate_manifest(self.manifest(), self.media)[0])

    def test_recording_metadata_is_required(self):
        self.write_media()
        manifest = self.manifest()
        manifest["appVersion"] = None
        manifest["capturedAt"] = "2026-02-30"
        errors = MEDIA.validate_manifest(manifest, self.media)
        self.assertTrue(any("appVersion" in error for error in errors))
        self.assertTrue(any("capturedAt" in error for error in errors))

    def test_names_cannot_escape_media_folder_or_repeat(self):
        self.write_media()
        errors = MEDIA.validate_manifest(self.manifest(("../hero", "hero", "hero")), self.media)
        self.assertTrue(any("invalid name" in error for error in errors))
        self.assertTrue(any("more than once" in error for error in errors))

    def test_dimensions_cannot_be_zero_strings_or_booleans(self):
        self.write_media()
        for invalid in (0, -1, "1920", True):
            with self.subTest(invalid=invalid):
                manifest = self.manifest()
                manifest["recordings"][0]["width"] = invalid
                self.assertIn("width", MEDIA.validate_manifest(manifest, self.media)[0])

    def test_wrong_asset_formats_are_rejected(self):
        self.write_media()
        (self.media / "hero-dark.mp4").write_text("not an MP4")
        (self.media / "hero-light.webp").write_bytes(b"\x89PNG\r\n\x1a\n")
        errors = MEDIA.validate_manifest(self.manifest(), self.media)
        self.assertTrue(any("MP4 header" in error for error in errors))
        self.assertTrue(any("WEBP header" in error for error in errors))

    def test_all_homepage_slots_can_be_required(self):
        for name in MEDIA.REQUIRED_RECORDINGS:
            self.write_media(name)
        manifest = self.manifest(sorted(MEDIA.REQUIRED_RECORDINGS))
        self.assertEqual(MEDIA.validate_manifest(manifest, self.media, True), [])


if __name__ == "__main__":
    unittest.main()
