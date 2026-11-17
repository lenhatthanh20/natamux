#!/usr/bin/env python3
"""Tests of npm-licenses-check.py. Run: python3 scripts/npm-licenses-check.test.py"""

import contextlib
import importlib.util
import io
import json
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("npm_licenses_check", HERE / "npm-licenses-check.py")
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)

CLEAN = {"MIT": [{"name": "react"}], "OFL-1.1": [{"name": "@fontsource-variable/inter"}]}


class NpmLicensesCheckTest(unittest.TestCase):
    def test_permissive_licenses_pass(self):
        self.assertEqual(check.problems(CLEAN), [])

    def test_gpl_family_fails_in_any_case_and_form(self):
        for license_id in ("GPL-3.0", "AGPL-3.0-only", "SSPL-1.0", "(MIT OR gpl-2.0)", "LGPL-2.1"):
            with self.subTest(license_id):
                self.assertEqual(check.problems({license_id: [{"name": "x"}]}), [f"x: {license_id}"])

    def test_a_signed_exception_passes(self):
        check.ALLOWED["x"] = "test"
        self.addCleanup(check.ALLOWED.pop, "x")
        self.assertEqual(check.problems({"GPL-3.0": [{"name": "x"}, {"name": "y"}]}), ["y: GPL-3.0"])

    def run_main(self, text):
        with tempfile.TemporaryDirectory() as folder:
            path = pathlib.Path(folder) / "licenses.json"
            path.write_text(text)
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                return check.main([str(path)])

    def test_main_exit_codes(self):
        self.assertEqual(self.run_main(json.dumps(CLEAN)), 0)
        self.assertEqual(self.run_main(json.dumps({"GPL-3.0": [{"name": "x"}]})), 1)
        self.assertEqual(self.run_main("not json"), 2)
        self.assertEqual(self.run_main("{}"), 2)
        self.assertEqual(self.run_main("[]"), 2)
        self.assertEqual(check.main([]), 2)
        self.assertEqual(check.main(["/no/such/file.json"]), 2)


if __name__ == "__main__":
    unittest.main()
