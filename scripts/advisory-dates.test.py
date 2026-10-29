#!/usr/bin/env python3
"""Tests of advisory-dates.py. Run: python3 scripts/advisory-dates.test.py"""

import datetime
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    "advisory_dates", pathlib.Path(__file__).with_name("advisory-dates.py")
)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

TODAY = datetime.date(2026, 10, 29)


def deny(*entries):
    body = "\n".join(f'  {{ id = "{i}", reason = "{r}" }},' for i, r in entries)
    return f"[advisories]\nignore = [\n{body}\n]\n"


def audit(*ids):
    body = "\n".join(f'  "{i}",' for i in ids)
    return f"[advisories]\nignore = [\n{body}\n]\n"


class Check(unittest.TestCase):
    def test_passes_with_future_dates_and_matching_ids(self):
        d = deny(("RUSTSEC-2026-0001", "no fix. Review by 2026-12-31."))
        self.assertEqual(mod.check(d, audit("RUSTSEC-2026-0001"), TODAY), [])

    def test_date_today_still_passes(self):
        d = deny(("RUSTSEC-2026-0001", "x. Review by 2026-10-29."))
        self.assertEqual(mod.check(d, "", TODAY), [])

    def test_fails_when_date_passed(self):
        d = deny(("RUSTSEC-2026-0001", "x. Review by 2026-10-28."))
        self.assertIn("has passed", mod.check(d, "", TODAY)[0])

    def test_fails_without_date(self):
        d = deny(("RUSTSEC-2026-0001", "no fix exists"))
        self.assertIn("no 'Review by", mod.check(d, "", TODAY)[0])

    def test_fails_on_impossible_date(self):
        d = deny(("RUSTSEC-2026-0001", "x. Review by 2026-13-40."))
        self.assertIn("bad date", mod.check(d, "", TODAY)[0])

    def test_fails_when_audit_id_missing_in_deny(self):
        d = deny(("RUSTSEC-2026-0001", "x. Review by 2026-12-31."))
        problems = mod.check(d, audit("RUSTSEC-2026-0001", "RUSTSEC-2026-0002"), TODAY)
        self.assertEqual(len(problems), 1)
        self.assertIn("RUSTSEC-2026-0002", problems[0])

    def test_repo_files_parse(self):
        deny_text = (mod.ROOT / "deny.toml").read_text()
        self.assertGreater(len(mod.DENY_ENTRY.findall(deny_text)), 5)


if __name__ == "__main__":
    unittest.main()
