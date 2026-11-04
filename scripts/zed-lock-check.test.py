#!/usr/bin/env python3
"""Tests of zed-lock-check.py. Run: python3 scripts/zed-lock-check.test.py"""

import contextlib
import importlib.util
import io
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("zed_lock_check", HERE / "zed-lock-check.py")
zed_lock_check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(zed_lock_check)

ZED = "git+https://github.com/lenhatthanh20/zed?rev=69c81337#69c81337"


def lock(*packages):
    """A Cargo.lock of (name, source) pairs; a None source is a workspace crate."""
    text = "version = 4\n"
    for name, source in packages:
        text += f'\n[[package]]\nname = "{name}"\nversion = "0.1.0"\n'
        if source:
            text += f'source = "{source}"\n'
    return text


PACKAGES = [
    ("gpui", ZED),
    ("editor", ZED),
    ("natamux-app", None),
    ("serde", "registry+https://github.com/rust-lang/crates.io-index"),
    ("font-kit", "git+https://github.com/lenhatthanh20/zed-font-kit?rev=1#1"),
]
TODAY = lock(*PACKAGES)
LIST = "# comment\ngpui\neditor\n\n"


class ZedLockCheckTest(unittest.TestCase):
    def test_the_exact_set_passes(self):
        self.assertEqual(zed_lock_check.problems(TODAY, LIST), [])

    def test_an_extra_zed_crate_fails(self):
        self.assertEqual(
            zed_lock_check.problems(lock(*PACKAGES, ("ui", ZED)), LIST),
            ["not in the list: ui"],
        )

    def test_a_missing_zed_crate_fails(self):
        self.assertEqual(
            zed_lock_check.problems(lock(("gpui", ZED)), LIST),
            ["listed but not in Cargo.lock: editor"],
        )

    def test_other_git_sources_with_zed_in_the_name_do_not_count(self):
        self.assertEqual(zed_lock_check.zed_crates(TODAY), {"gpui", "editor"})

    def test_main_reads_the_files_and_exits_1_on_a_problem(self):
        with tempfile.TemporaryDirectory() as folder:
            lock_file, list_file = pathlib.Path(folder, "Cargo.lock"), pathlib.Path(folder, "list")
            lock_file.write_text(TODAY)
            list_file.write_text(LIST)
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(zed_lock_check.main([str(lock_file), str(list_file)]), 0)
                list_file.write_text("gpui\n")
                self.assertEqual(zed_lock_check.main([str(lock_file), str(list_file)]), 1)


if __name__ == "__main__":
    unittest.main()
