#!/usr/bin/env python3
"""Publish source hashes and cuts without local paths or daemon access details."""

import argparse
from datetime import date
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

SLOTS = ("hero", "status", "progress", "review", "settings", "editor")
THEMES = ("dark", "light")


def sha256(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def public_filename(name):
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*", name):
        raise ValueError("Public filenames must contain only letters, numbers, dots, dashes, and underscores.")
    return name


def image_dimensions(path):
    result = subprocess.check_output(["/usr/bin/sips", "-g", "pixelWidth", "-g", "pixelHeight", str(path)], text=True)
    return {key: int(re.search(rf"pixel{label}: (\d+)", result)[1])
            for key, label in (("width", "Width"), ("height", "Height"))}


def raw_source(info, expected):
    if (info["width"], info["height"]) != (3840, 2160):
        raise ValueError("Every source must be an original 3840x2160 capture.")
    path = Path(info["path"])
    if sha256(path) != expected:
        raise ValueError(f"Original source changed: {path.name}")
    return {"filename": public_filename(path.name), "sha256": expected, "width": 3840, "height": 2160}


def probe_video(path):
    binary = os.environ.get("NATAMUX_CAPTURE_FFPROBE") or shutil.which("ffprobe")
    if not binary:
        raise ValueError("Set NATAMUX_CAPTURE_FFPROBE to an existing ffprobe binary.")
    result = json.loads(subprocess.check_output([
        binary, "-v", "error", "-show_entries",
        "stream=codec_type,codec_name,width,height,r_frame_rate,pix_fmt,nb_frames:format=duration,size",
        "-of", "json", str(path),
    ], text=True))
    if any(stream["codec_type"] == "audio" for stream in result["streams"]):
        raise ValueError(f"Unexpected audio: {path.name}")
    video = next(stream for stream in result["streams"] if stream["codec_type"] == "video")
    return {
        "filename": path.name, "sha256": sha256(path), "bytes": path.stat().st_size,
        "width": video["width"], "height": video["height"], "codec": video["codec_name"],
        "pixelFormat": video["pix_fmt"], "frameRate": video["r_frame_rate"],
        "frames": int(video["nb_frames"]), "duration": float(result["format"]["duration"]),
        "audio": False,
    }


def find_cut(name, checksum, reports):
    for report in reports:
        clip = next((item for item in report["cuts"]["clips"] if item["name"] == name), None)
        if clip is None:
            continue
        for item in report["outputs"]:
            path = Path(item["path"])
            if path.name == f"{name}.mp4" and path.is_file() and sha256(path) == checksum:
                return clip, report
    raise ValueError(f"No matching source/cut provenance for {name}")


def recording(name, media, reports):
    video = probe_video(media / f"{name}.mp4")
    clip, report = find_cut(name, video["sha256"], reports)
    poster = media / f"{name}.webp"
    poster_draft = next(Path(item["path"]) for item in report["outputs"] if Path(item["path"]).name == poster.name)
    if sha256(poster_draft) != sha256(poster):
        raise ValueError(f"Poster differs from the selected cut report: {poster.name}")
    ranges = clip.get("ranges", [[clip.get("start"), clip.get("end")]])
    return {
        "name": name, "raw": raw_source(report["source"], report["sourceSha256"]),
        "cuts": ranges, "crop": clip.get("crop", [0, 0, 3840, 2160]),
        "posterAt": clip.get("posterAt", ranges[0][0]), "video": video,
        "poster": {"filename": poster.name, "sha256": sha256(poster), "bytes": poster.stat().st_size,
                   **image_dimensions(poster)},
    }


def screenshot(path, postprocess):
    sidecar = load(postprocess / "docs" / (path.name + ".json"))
    draft = postprocess / "docs" / path.name
    if sha256(path) != sha256(draft):
        raise ValueError(f"Screenshot differs from its reviewed draft: {path.name}")
    entry = {
        "filename": path.name, "sha256": sha256(path), "bytes": path.stat().st_size,
        **image_dimensions(path),
        "raw": raw_source(sidecar["source"], sidecar["sourceSha256"]),
        "crop": sidecar["crop"],
    }
    if "at" in sidecar:
        entry["frameAt"] = sidecar["at"]
    return entry


def validate_refresh(previous, current):
    for key in ("appVersion", "releaseURL", "releaseAsset", "capturedAt"):
        if previous[key] != current[key]:
            raise ValueError("A refreshed report must describe the same release and capture date.")
    for section, key in (("recordings", "name"), ("screenshots", "filename")):
        entries = {item[key]: item for item in current[section]}
        if any(entries.get(item[key]) != item for item in previous[section]):
            raise ValueError("Refresh can add verified captures but cannot change or remove earlier entries.")
    available = {item["filename"] for item in current["screenshots"]}
    still_pending = set(current["pendingScreenshots"])
    if any(name not in available | still_pending for name in previous["pendingScreenshots"]):
        raise ValueError("A pending screenshot needs a verified file before it can be marked complete.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--postprocess", type=Path, required=True)
    parser.add_argument("--media", type=Path, default=Path("site/public/media/native"))
    parser.add_argument("--screens", type=Path, default=Path("site/public/screens/native"))
    parser.add_argument("--app-version", required=True)
    parser.add_argument("--release-url", required=True)
    parser.add_argument("--release-asset", default="Natamux-mac-arm64.dmg")
    parser.add_argument("--release-asset-sha256", required=True)
    parser.add_argument("--captured-at", required=True)
    parser.add_argument("--pending-scene", action="append", default=[])
    parser.add_argument("--refresh", action="store_true", help="append verified captures to this release's existing report")
    args = parser.parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][A-Za-z0-9.-]+)?", args.app_version):
        raise ValueError("App version must be a release version.")
    release_pattern = rf"https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/releases/tag/v{re.escape(args.app_version)}"
    if not re.fullmatch(release_pattern, args.release_url):
        raise ValueError("Release URL must be the public GitHub release tag for this app version.")
    public_filename(args.release_asset)
    if date.fromisoformat(args.captured_at).isoformat() != args.captured_at:
        raise ValueError("Capture date must use YYYY-MM-DD.")
    if any(not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name) for name in args.pending_scene):
        raise ValueError("Pending scene names must use lowercase letters, numbers, and dashes.")
    if not re.fullmatch(r"[a-f0-9]{64}", args.release_asset_sha256):
        raise ValueError("Release asset SHA-256 must be 64 lowercase hex characters.")
    target = args.media / "capture-provenance.json"
    previous = load(target) if target.exists() else None
    if previous and not args.refresh:
        raise ValueError("Public capture provenance already exists; keep it and choose a new capture folder.")
    reports = [load(path) for path in args.postprocess.rglob("capture-provenance.json")]
    names = [f"{name}-{theme}" for name in SLOTS for theme in THEMES]
    extras = sorted({path.stem.rsplit("-", 1)[0] for path in args.media.glob("*.mp4") if path.stem not in names})
    names += [f"{name}-{theme}" for name in extras for theme in THEMES]
    recordings = [recording(name, args.media, reports) for name in names]
    for index in range(0, len(recordings), 2):
        dark, light = [item["video"] for item in recordings[index:index + 2]]
        if any(dark[key] != light[key] for key in ("width", "height", "frames", "frameRate")):
            raise ValueError("A theme pair has different dimensions or frame counts.")
    result = {
        "appVersion": args.app_version, "releaseURL": args.release_url,
        "releaseAsset": {"filename": args.release_asset, "sha256": args.release_asset_sha256},
        "capturedAt": args.captured_at, "display": "Dell P3223QE",
        "capture": "Native macOS screencapture", "rawWidth": 3840, "rawHeight": 2160,
        "scriptedAgents": True, "recordings": recordings,
        "captureComplete": not args.pending_scene,
        "pendingScreenshots": [f"{name}-{theme}.webp" for name in args.pending_scene for theme in THEMES],
        "openGraph": screenshot(args.media / "og.jpg", args.postprocess),
        "screenshots": [screenshot(path, args.postprocess) for path in sorted(args.screens.glob("*.webp"))],
    }
    encoded = json.dumps(result, indent=2) + "\n"
    if "/private/" in encoded or "/Users/" in encoded or "NATAMUX_TOKEN" in encoded:
        raise ValueError("Public provenance contains private execution details.")
    if previous:
        validate_refresh(previous, result)
    with target.open("w" if previous else "x", encoding="utf-8") as output:
        output.write(encoded)
    print(f"Wrote {len(recordings)} recordings and {len(result['screenshots'])} screenshots.")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"native-record provenance: {error}", file=sys.stderr)
        sys.exit(1)
