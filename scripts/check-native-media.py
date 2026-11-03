#!/usr/bin/env python3
"""Check that every native recording advertised by the site has both theme variants."""

import argparse
from datetime import date
import json
from pathlib import Path
import re
import sys


REQUIRED_RECORDINGS = {"hero", "status", "progress", "review", "settings", "editor"}


def validate_manifest(manifest, media_dir, require_all=False):
    errors = []
    if not isinstance(manifest, dict) or not isinstance(manifest.get("recordings"), list):
        return ["Manifest needs a recordings array."]
    recordings = manifest["recordings"]
    if recordings:
        version = manifest.get("appVersion")
        if not isinstance(version, str) or not re.fullmatch(
            r"v?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version
        ):
            errors.append("Recorded media needs its actual appVersion, such as 1.0.17.")
        captured_at = manifest.get("capturedAt")
        try:
            if not isinstance(captured_at, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}", captured_at):
                raise ValueError
            date.fromisoformat(captured_at)
        except ValueError:
            errors.append("Recorded media needs a valid capturedAt date (YYYY-MM-DD).")
    names = set()
    for index, recording in enumerate(recordings):
        if not isinstance(recording, dict):
            errors.append(f"Recording {index + 1} must be an object.")
            continue
        name = recording.get("name")
        if not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9-]*", name):
            errors.append(f"Recording {index + 1} has an invalid name.")
            continue
        if name in names:
            errors.append(f"Recording {name} appears more than once.")
        names.add(name)
        for dimension in ("width", "height"):
            value = recording.get(dimension)
            if type(value) is not int or value <= 0:
                errors.append(f"Recording {name} needs a positive integer {dimension}.")
        for theme in ("dark", "light"):
            for suffix in ("mp4", "webp"):
                path = media_dir / f"{name}-{theme}.{suffix}"
                try:
                    with path.open("rb") as source:
                        header = source.read(12)
                except OSError:
                    errors.append(f"Recording {name} is missing {path.name}.")
                    continue
                valid = (
                    len(header) == 12 and header[4:8] == b"ftyp"
                    if suffix == "mp4"
                    else header[:4] == b"RIFF" and header[8:12] == b"WEBP"
                )
                if not valid:
                    errors.append(f"File {path.name} does not have a {suffix.upper()} header.")
    if require_all:
        missing = REQUIRED_RECORDINGS - names
        if missing:
            errors.append(f"Native recordings are still pending: {', '.join(sorted(missing))}.")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site", type=Path, default=Path(__file__).resolve().parents[1] / "site")
    parser.add_argument("--require-all", action="store_true", help="reject pending homepage recordings")
    args = parser.parse_args()
    try:
        manifest = json.loads((args.site / "src/data/native-recordings.json").read_text())
    except (OSError, ValueError) as error:
        print(f"Cannot read native media manifest: {error}", file=sys.stderr)
        return 1
    errors = validate_manifest(manifest, args.site / "public/media/native", args.require_all)
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    count = len(manifest["recordings"])
    print(f"Native media manifest is valid: {count} recordings.")
    if count == 0:
        print("The site will display recording-pending panels.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
