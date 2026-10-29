#!/usr/bin/env python3
"""Checks the advisory ignores of deny.toml. Run: python3 scripts/advisory-dates.py

Fails when a reason has no "Review by YYYY-MM-DD." date, a date has passed, or
.cargo/audit.toml ignores an id that deny.toml does not.
"""

import datetime
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DENY_ENTRY = re.compile(r'\{\s*id\s*=\s*"(RUSTSEC-[\d-]+)"\s*,\s*reason\s*=\s*"([^"]*)"')
AUDIT_ID = re.compile(r'^\s*"(RUSTSEC-[\d-]+)"\s*,?\s*(#.*)?$', re.MULTILINE)
REVIEW = re.compile(r"Review by (\d{4}-\d{2}-\d{2})\.\s*$")


def check(deny_text, audit_text, today):
    problems = []
    entries = DENY_ENTRY.findall(deny_text)
    for ident, reason in entries:
        match = REVIEW.search(reason)
        if not match:
            problems.append(f"{ident}: reason has no 'Review by YYYY-MM-DD.' date")
            continue
        try:
            due = datetime.date.fromisoformat(match.group(1))
        except ValueError:
            problems.append(f"{ident}: bad date {match.group(1)}")
            continue
        if due < today:
            problems.append(f"{ident}: review date {due} has passed")
    known = {ident for ident, _ in entries}
    for ident in AUDIT_ID.findall(audit_text):
        if ident[0] not in known:
            problems.append(f"{ident[0]}: in .cargo/audit.toml but not in deny.toml")
    return problems


def main():
    deny = (ROOT / "deny.toml").read_text()
    audit = (ROOT / ".cargo" / "audit.toml").read_text()
    problems = check(deny, audit, datetime.date.today())
    for problem in problems:
        print(f"advisory-dates: {problem}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
