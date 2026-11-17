#!/usr/bin/env python3
"""Fails when a package of the web bundle has a GPL, AGPL or SSPL license.

Usage: npm-licenses-check.py <pnpm licenses list --json output>

The input maps a license id to a list of packages. A package listed in ALLOWED (owner-signed)
passes. Exit 0 when clean, 1 when a package fails, 2 on bad input.
"""

import json
import sys
from pathlib import Path

BAD = ("GPL", "AGPL", "SSPL")
# Owner-signed exceptions as "name": "reason". Empty on purpose.
ALLOWED = {}


def problems(licenses):
    found = []
    for license_id, packages in sorted(licenses.items()):
        if not any(word in license_id.upper() for word in BAD):
            continue
        for package in packages:
            name = package.get("name", "?")
            if name not in ALLOWED:
                found.append(f"{name}: {license_id}")
    return found


def main(argv):
    if len(argv) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        licenses = json.loads(Path(argv[0]).read_text())
    except (OSError, ValueError) as error:
        print(f"cannot read {argv[0]}: {error}", file=sys.stderr)
        return 2
    if not isinstance(licenses, dict) or not licenses:
        print("input is not a license map, or it is empty", file=sys.stderr)
        return 2
    found = problems(licenses)
    count = sum(len(packages) for packages in licenses.values())
    for line in found:
        print(f"bad license: {line}")
    print(f"{count} packages, {len(licenses)} licenses, {len(found)} bad")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
