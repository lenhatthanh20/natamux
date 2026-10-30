#!/usr/bin/env python3
"""Exact pixel compare of two folders of CI shots (PNG, 8-bit RGB or RGBA, not interlaced).

Usage: shots-exact.py <base_dir> <new_dir> [--control <dir>] [--out <diff_dir>] [--only <prefix>...]

Every shot of new_dir is compared with the shot of the same name in base_dir. A shot passes
when every pixel is equal (alpha ignored). With --control (a second run of the same code as
base), a pixel that differs between base and control is moving: it may differ in new too.
The window-* shots (drawn by the OS, with the menu bar clock) are skipped. --only keeps the
shots whose name starts with one of the prefixes. --out writes diff-<name>.png for each failed
shot: the differing pixels in red over a dimmed copy of the new shot.

Exit 0 only when every shot matches. A shot with no base, a base shot with no new shot (a
partial run) or another size fails.
"""

import argparse
import struct
import sys
import zlib
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
# Differing pixels this close to each other are reported as one box.
BOX_GAP = 8


def decode_png(data):
    """Returns (width, height, rows); each row is the RGB bytes of one pixel row."""
    if data[:8] != PNG_SIGNATURE:
        raise ValueError("not a PNG file")
    pos, head, idat = 8, None, []
    while pos < len(data):
        size, kind = struct.unpack_from(">I4s", data, pos)
        body = data[pos + 8 : pos + 8 + size]
        pos += 12 + size
        if kind == b"IHDR":
            head = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
    if head is None:
        raise ValueError("no IHDR chunk")
    width, height, depth, colour, _, _, interlace = head
    if depth != 8 or colour not in (2, 6) or interlace:
        raise ValueError(f"unsupported PNG: depth {depth}, colour {colour}, interlace {interlace}")
    bpp = 3 if colour == 2 else 4
    stride = width * bpp
    raw = zlib.decompress(b"".join(idat))
    if len(raw) < height * (stride + 1):
        raise ValueError("pixel data is too short")
    rows, prev = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind = raw[start]
        line = bytearray(raw[start + 1 : start + 1 + stride])
        if kind == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif kind == 2:
            line = bytearray((a + b) & 255 for a, b in zip(line, prev))
        elif kind == 3:
            for i in range(stride):
                left = line[i - bpp] if i >= bpp else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 255
        elif kind == 4:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        elif kind != 0:
            raise ValueError(f"bad filter {kind} in row {y}")
        prev = line
        if bpp == 4:
            rgb = bytearray(width * 3)
            rgb[0::3], rgb[1::3], rgb[2::3] = line[0::4], line[1::4], line[2::4]
            rows.append(bytes(rgb))
        else:
            rows.append(bytes(line))
    return width, height, rows


def read_png(path):
    try:
        return decode_png(Path(path).read_bytes())
    except ValueError as error:
        raise ValueError(f"{path}: {error}") from None


def encode_png(width, rows):
    """An RGB PNG of the rows (no filter)."""

    def chunk(kind, body):
        crc = struct.pack(">I", zlib.crc32(kind + body))
        return struct.pack(">I", len(body)) + kind + body + crc

    head = struct.pack(">IIBBBBB", width, len(rows), 8, 2, 0, 0, 0)
    raw = b"".join(b"\x00" + row for row in rows)
    return (
        PNG_SIGNATURE
        + chunk(b"IHDR", head)
        + chunk(b"IDAT", zlib.compress(raw, 6))
        + chunk(b"IEND", b"")
    )


def differing(a_rows, b_rows):
    """The set of (x, y) where two images of the same size differ."""
    out = set()
    for y, (a, b) in enumerate(zip(a_rows, b_rows)):
        if a == b:
            continue
        for i in range(0, len(a), 3):
            if a[i : i + 3] != b[i : i + 3]:
                out.add((i // 3, y))
    return out


def boxes(pixels, gap=BOX_GAP):
    """Groups pixels into boxes (x0, y0, x1, y1), joining pixels at most `gap` apart."""
    found = []
    for x, y in sorted(pixels, key=lambda p: (p[1], p[0])):
        for box in found:
            if box[0] - gap <= x <= box[2] + gap and box[1] - gap <= y <= box[3] + gap:
                box[0], box[1] = min(box[0], x), min(box[1], y)
                box[2], box[3] = max(box[2], x), max(box[3], y)
                break
        else:
            found.append([x, y, x, y])
    # Two boxes can grow into each other; join them until none touch.
    joined = True
    while joined:
        joined = False
        for i in range(len(found)):
            for j in range(i + 1, len(found)):
                a, b = found[i], found[j]
                near_x = a[0] - gap <= b[2] and b[0] - gap <= a[2]
                if near_x and a[1] - gap <= b[3] and b[1] - gap <= a[3]:
                    found[i] = [min(a[0], b[0]), min(a[1], b[1]), max(a[2], b[2]), max(a[3], b[3])]
                    del found[j]
                    joined = True
                    break
            if joined:
                break
    return [tuple(box) for box in sorted(found, key=lambda b: (b[1], b[0]))]


def compare(base, new, control=None):
    """Returns (problem or None, bad pixels, moving pixels). Bad pixels differ outside the
    moving ones; moving pixels differ between base and control."""
    (bw, bh, brows), (nw, nh, nrows) = base, new
    if (bw, bh) != (nw, nh):
        return f"size {nw}x{nh}, base {bw}x{bh}", set(), set()
    diff = differing(brows, nrows)
    moving = set()
    if control is not None and diff:
        cw, ch, crows = control
        if (cw, ch) == (bw, bh):
            moving = differing(brows, crows)
    bad = diff - moving
    return ("differs" if bad else None), bad, moving


def diff_image(new, bad):
    """The new shot dimmed, with the bad pixels in red."""
    width, _, rows = new
    out = []
    for y, row in enumerate(rows):
        line = bytearray(c // 3 + 170 for c in row)
        for x in range(width):
            if (x, y) in bad:
                line[x * 3 : x * 3 + 3] = b"\xff\x00\x00"
        out.append(bytes(line))
    return encode_png(width, out)


def inside(path, folder):
    path, folder = path.resolve(), folder.resolve()
    return path == folder or folder in path.parents


def main(argv):
    parser = argparse.ArgumentParser(description="Exact pixel compare of two folders of CI shots.")
    parser.add_argument("base_dir", type=Path)
    parser.add_argument("new_dir", type=Path)
    parser.add_argument("--control", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--only", nargs="+", default=[])
    args = parser.parse_args(argv)

    inputs = [args.base_dir, args.new_dir] + ([args.control] if args.control else [])
    for folder in inputs:
        if not folder.is_dir():
            print(f"error: {folder} is not a folder", file=sys.stderr)
            return 2
    if args.out:
        if any(inside(args.out, folder) for folder in inputs):
            print(f"error: --out {args.out} is inside an input folder", file=sys.stderr)
            return 2
        args.out.mkdir(parents=True, exist_ok=True)

    def wanted(name):
        return not name.startswith("window-") and (
            not args.only or name.startswith(tuple(args.only))
        )

    checked = failed = 0
    for shot in sorted(args.base_dir.glob("*.png")):
        if wanted(shot.stem) and not (args.new_dir / shot.name).is_file():
            print(f"NO NEW   {shot.stem}")
            checked += 1
            failed += 1
    for shot in sorted(args.new_dir.glob("*.png")):
        name = shot.stem
        if not wanted(name):
            continue
        checked += 1
        base_file = args.base_dir / shot.name
        if not base_file.is_file():
            print(f"NO BASE  {name}")
            failed += 1
            continue
        if base_file.read_bytes() == shot.read_bytes():
            print(f"ok       {name}")
            continue
        base, new = read_png(base_file), read_png(shot)
        control = None
        if args.control and (args.control / shot.name).is_file():
            control = read_png(args.control / shot.name)
        problem, bad, moving = compare(base, new, control)
        note = f" ({len(moving)} moving px)" if moving else ""
        if problem is None:
            print(f"ok       {name}{note}")
            continue
        failed += 1
        if not bad:
            print(f"DIFFERS  {name}: {problem}")
            continue
        found = " ".join(f"({x0},{y0})-({x1},{y1})" for x0, y0, x1, y1 in boxes(bad))
        print(f"DIFFERS  {name}: {len(bad)} px{note}, boxes {found}")
        if args.out:
            (args.out / f"diff-{shot.name}").write_bytes(diff_image(new, bad))
    print(f"{checked} shots, {failed} failed")
    # No shot compared is a failure too: a wrong folder or --only must not pass.
    return 1 if failed or not checked else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
