#!/usr/bin/env python3
"""Record a physical 4K display, verify the source, and cut explicit clip ranges."""

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent
SOURCE_SIZE = (3840, 2160)


def run(command):
    subprocess.run([str(value) for value in command], check=True)


def metadata(mode, *arguments):
    result = subprocess.run(
        ["/usr/bin/swift", "-module-cache-path", str(Path(tempfile.gettempdir()) / "natamux-native-record-module-cache"),
         str(HERE / "native-record.swift"), mode, *map(str, arguments)],
        capture_output=True, text=True,
    )
    if result.returncode:
        raise ValueError(result.stderr.strip() or "Native video inspection failed.")
    return json.loads(result.stdout)


def write_new(path, value):
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2)
        output.write("\n")


def require_source_size(info):
    size = (info.get("width"), info.get("height"))
    if size != SOURCE_SIZE:
        raise ValueError(f"Source is {size[0]}x{size[1]}; require an original 3840x2160 recording. File kept.")


def require_source(info):
    require_source_size(info)
    if not math.isfinite(info.get("duration", 0)) or info.get("duration", 0) <= 0:
        raise ValueError("Source duration must be positive.")


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def select_display(status, index):
    displays = status.get("displays", [])
    if not displays:
        raise ValueError("No desktop displays are visible. Run from Terminal in the logged-in Mac desktop session.")
    if not status.get("screenRecordingAllowed"):
        raise ValueError("Allow the terminal app in System Settings > Privacy & Security > Screen & System Audio Recording, then reopen that terminal app.")
    selected = next((display for display in displays if display["index"] == index), None)
    if selected is None:
        raise ValueError(f"Display {index} is not listed by preflight.")
    if (selected["pixelWidth"], selected["pixelHeight"]) != SOURCE_SIZE:
        raise ValueError("Selected display is not 3840x2160. Run preflight and select the Dell 4K display.")
    if "dell" not in selected["name"].lower():
        raise ValueError("Selected display is not named Dell. Verify the physical display in preflight before recording.")
    return selected


def record(args):
    output = args.output.resolve()
    sidecar = output.with_suffix(output.suffix + ".json")
    if output.suffix.lower() != ".mov":
        raise ValueError("Record to a .mov file.")
    if output.exists() or sidecar.exists():
        raise ValueError("Output or its metadata already exists; choose a new filename.")
    selected = select_display(metadata("preflight"), args.display)
    output.parent.mkdir(parents=True, exist_ok=True)
    command = ["/usr/sbin/screencapture", "-x", "-v", f"-D{args.display}", f"-V{args.seconds}", "-C", "-k", output]
    print(f"Recording {selected['name']} for {args.seconds}s to {output}", flush=True)
    run(command)
    info = metadata("inspect", output)
    provenance = {
        "recordedAt": datetime.now(timezone.utc).isoformat(),
        "capture": "native macOS screencapture; full physical display; no audio",
        "display": selected,
        "command": list(map(str, command)),
        "source": info,
        "sha256": digest(output),
    }
    write_new(sidecar, provenance)
    require_source(info)
    print(json.dumps(provenance, indent=2))


def finite_number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def clip_ranges(clip):
    return clip.get("ranges", [[clip.get("start"), clip.get("end")]])


def validate_clips(manifest, duration):
    if not isinstance(manifest, dict) or manifest.get("version") != 1:
        raise ValueError("Cut manifest must have version 1.")
    clips = manifest.get("clips")
    if not isinstance(clips, list) or not clips:
        raise ValueError("Cut manifest needs a nonempty clips array.")
    names = set()
    for clip in clips:
        if not isinstance(clip, dict):
            raise ValueError("Each clip must be an object.")
        name = clip.get("name", "")
        if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name) or name in names:
            raise ValueError("Clip names must be unique lowercase filenames, without an extension.")
        names.add(name)
        ranges = clip_ranges(clip)
        if not isinstance(ranges, list) or not ranges:
            raise ValueError(f"{name}: ranges must be a nonempty list of [start, end] pairs.")
        previous_end = 0
        for item in ranges:
            if not isinstance(item, list) or len(item) != 2:
                raise ValueError(f"{name}: each range must be [start, end].")
            start, end = item
            if not finite_number(start) or not finite_number(end) or not previous_end <= start < end <= duration:
                raise ValueError(f"{name}: ranges must follow the recording's order and stay inside it.")
            previous_end = end
        poster_at = clip.get("posterAt", ranges[0][0])
        if not finite_number(poster_at) or not any(start <= poster_at < end for start, end in ranges):
            raise ValueError(f"{name}: poster time must fall inside the clip.")
        crop = clip.get("crop", [0, 0, *SOURCE_SIZE])
        if not isinstance(crop, list) or len(crop) != 4 or any(type(v) is not int for v in crop):
            raise ValueError(f"{name}: crop must be four whole pixels: x, y, width, height.")
        x, y, width, height = crop
        if x < 0 or y < 0 or width <= 0 or height <= 0 or x + width > SOURCE_SIZE[0] or y + height > SOURCE_SIZE[1]:
            raise ValueError(f"{name}: crop falls outside the original display recording.")
        if any(value % 2 for value in crop):
            raise ValueError(f"{name}: use even crop values for H.264.")
        output_width = clip.get("width", 1920)
        if type(output_width) is not int or output_width <= 0 or output_width % 2 or output_width > width:
            raise ValueError(f"{name}: output width must be positive, even, and no larger than the crop.")
    return clips


def encoder(source, output, options):
    binary = os.environ.get("NATAMUX_CAPTURE_FFMPEG") or shutil.which("ffmpeg")
    if binary:
        run([binary, "-hide_banner", "-loglevel", "error", "-n", "-threads", "2",
             "-filter_threads", "2", "-i", source, *options, output])
        return
    if not shutil.which("docker"):
        raise ValueError("Encoding needs existing ffmpeg or Docker. No packages are installed by this kit.")
    image = "linuxserver/ffmpeg:latest"
    subprocess.run(["docker", "image", "inspect", image], check=True, stdout=subprocess.DEVNULL)
    run(["bash", HERE / "ffm.sh", source, output, *options])


def movie_options(clip, video_filter):
    ranges = clip_ranges(clip)
    if len(ranges) == 1:
        start, end = ranges[0]
        return ["-ss", str(start), "-t", str(end - start), "-vf",
                f"{video_filter},fps=30,tpad=stop_mode=clone:stop_duration=2,format=yuv420p"]
    labels = "".join(f"[take{index}]" for index in range(len(ranges)))
    filters = [f"[0:v]split={len(ranges)}{labels}"]
    for index, (start, end) in enumerate(ranges):
        frames = round((end - start) * 30)
        filters.append(
            f"[take{index}]trim=start={start}:end={end},setpts=PTS-STARTPTS,fps=30,"
            f"tpad=stop_mode=clone:stop_duration={end - start},trim=end_frame={frames}[part{index}]"
        )
    parts = "".join(f"[part{index}]" for index in range(len(ranges)))
    filters.append(f"{parts}concat=n={len(ranges)}:v=1:a=0,{video_filter},format=yuv420p[movie]")
    return ["-filter_complex", ";".join(filters), "-map", "[movie]"]


def encode(args):
    source = args.source.resolve()
    info = metadata("inspect", source)
    require_source(info)
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    clips = validate_clips(manifest, info["duration"])
    destination = args.output.resolve()
    targets = [destination / (clip["name"] + suffix) for clip in clips for suffix in (".mp4", ".webp")]
    report = destination / "capture-provenance.json"
    if any(target.exists() for target in [*targets, report]):
        raise ValueError("An output already exists; choose a fresh output folder. Originals are never replaced.")
    destination.mkdir(parents=True, exist_ok=True)
    for clip in clips:
        name = clip["name"]
        crop = clip.get("crop", [0, 0, *SOURCE_SIZE])
        x, y, width, height = crop
        output_width = clip.get("width", 1920)
        video_filter = f"crop={width}:{height}:{x}:{y},scale={output_width}:-2:flags=lanczos,setsar=1"
        encoder(source, destination / f"{name}.mp4", [
            # A screen capture can hold its last sample without another timestamped frame.
            *movie_options(clip, video_filter), "-an", "-c:v", "libx264",
            "-preset", "slow", "-crf", str(args.crf), "-profile:v", "high", "-pix_fmt", "yuv420p",
            "-threads", "4", "-movflags", "+faststart",
        ])
        encoder(source, destination / f"{name}.webp", [
            "-ss", str(clip.get("posterAt", clip_ranges(clip)[0][0])), "-frames:v", "1", "-vf", video_filter,
            "-an", "-c:v", "libwebp", "-quality", "82",
        ])
        print(f"Created {name}.mp4 and {name}.webp", flush=True)
    outputs = []
    for target in targets:
        entry = {"path": str(target), "bytes": target.stat().st_size}
        if target.suffix == ".mp4":
            entry.update(metadata("inspect", target))
        outputs.append(entry)
    write_new(report, {
        "source": info, "sourceSha256": digest(source), "cuts": manifest,
        "encoding": {"codec": "H.264 High", "fps": 30, "pixelFormat": "yuv420p", "crf": args.crf},
        "outputs": outputs,
    })


def still(args):
    source = args.source.resolve()
    info = metadata("inspect", source)
    require_source(info)
    crop = args.crop
    validate_clips({"version": 1, "clips": [{
        "name": "still", "start": args.at, "end": info["duration"],
        "crop": crop, "width": args.width,
    }]}, info["duration"])
    output = args.output.resolve()
    sidecar = output.with_suffix(output.suffix + ".json")
    if output.suffix.lower() != ".webp":
        raise ValueError("Export a still to .webp.")
    if output.exists() or sidecar.exists():
        raise ValueError("Output or its metadata already exists; choose a new filename.")
    output.parent.mkdir(parents=True, exist_ok=True)
    x, y, width, height = crop
    encoder(source, output, [
        "-ss", str(args.at), "-frames:v", "1", "-vf",
        f"crop={width}:{height}:{x}:{y},scale={args.width}:-2:flags=lanczos,setsar=1",
        "-an", "-c:v", "libwebp", "-quality", "82",
    ])
    write_new(sidecar, {
        "source": info, "sourceSha256": digest(source), "at": args.at,
        "crop": crop, "width": args.width, "output": str(output), "bytes": output.stat().st_size,
    })


def image(args):
    source = args.source.resolve()
    info = metadata("image", source)
    require_source_size(info)
    validate_clips({"version": 1, "clips": [{
        "name": "image", "start": 0, "end": 1,
        "crop": args.crop, "width": args.width,
    }]}, 1)
    output = args.output.resolve()
    sidecar = output.with_suffix(output.suffix + ".json")
    if output.suffix.lower() != ".webp":
        raise ValueError("Export a screenshot to .webp.")
    if output.exists() or sidecar.exists():
        raise ValueError("Output or its metadata already exists; choose a new filename.")
    binary = shutil.which("cwebp")
    if not binary:
        raise ValueError("Screenshot encoding needs an existing cwebp. No package is installed by this kit.")
    output.parent.mkdir(parents=True, exist_ok=True)
    run([binary, "-quiet", "-q", "82", "-m", "6", "-crop", *args.crop,
         "-resize", args.width, "0", source, "-o", output])
    write_new(sidecar, {
        "source": info, "sourceSha256": digest(source), "crop": args.crop,
        "width": args.width, "output": metadata("image", output),
    })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("preflight", help="list desktop displays and existing permissions; never prompts")
    inspect = commands.add_parser("inspect", help="read the dimensions and duration of a real recording")
    inspect.add_argument("source", type=Path)
    recording = commands.add_parser("record", help="record the full Dell 3840x2160 display; keep its original")
    recording.add_argument("output", type=Path)
    recording.add_argument("--seconds", type=int, default=90, choices=range(1, 3601), metavar="1..3600")
    recording.add_argument("--display", type=int, default=2)
    frame = commands.add_parser("still", help="extract a genuine native frame for a docs screenshot")
    frame.add_argument("source", type=Path)
    frame.add_argument("output", type=Path)
    frame.add_argument("--at", type=float, required=True)
    frame.add_argument("--crop", type=int, nargs=4, default=[0, 0, *SOURCE_SIZE], metavar=("X", "Y", "W", "H"))
    frame.add_argument("--width", type=int, default=1600)
    screenshot = commands.add_parser("image", help="crop and encode an original full-Dell 4K screenshot")
    screenshot.add_argument("source", type=Path)
    screenshot.add_argument("output", type=Path)
    screenshot.add_argument("--crop", type=int, nargs=4, default=[0, 0, *SOURCE_SIZE], metavar=("X", "Y", "W", "H"))
    screenshot.add_argument("--width", type=int, default=1600)
    cuts = commands.add_parser("encode", help="cut an original 4K take using an explicit JSON timing/crop manifest")
    cuts.add_argument("source", type=Path)
    cuts.add_argument("manifest", type=Path)
    cuts.add_argument("output", type=Path)
    cuts.add_argument("--crf", type=int, default=22, choices=range(0, 52), metavar="0..51")
    args = parser.parse_args()
    if args.command == "preflight":
        print(json.dumps(metadata("preflight"), indent=2))
    elif args.command == "inspect":
        print(json.dumps(metadata("inspect", args.source), indent=2))
    elif args.command == "record":
        record(args)
    elif args.command == "still":
        still(args)
    elif args.command == "image":
        image(args)
    else:
        encode(args)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"native-record: {error}", file=sys.stderr)
        sys.exit(1)
