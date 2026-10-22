#!/usr/bin/env python3
"""Compares a screenshot of the app with its baseline, and writes a diff image.

Usage: compare-shots.py [--report-only] <shot.bmp> <baseline.bmp> <diff.bmp>

A shot fails when more than 0.5% of its pixels differ by more than 16 in a channel, or when
the sizes differ. The diff image shows the differing pixels in red over a dimmed copy of the
shot. --report-only prints the result and exits 0, until the owner has signed the baselines off.
"""

import struct
import sys

# Share of pixels that may differ.
MAX_SHARE = 0.005
# A channel must differ by more than this for the pixel to count.
CHANNEL_LIMIT = 16


def read_bmp(data):
    """Returns (width, height, rows): rows are top-down, each one RGB bytes of the pixel row."""
    if data[:2] != b"BM":
        raise ValueError("not a BMP file")
    offset = struct.unpack_from("<I", data, 10)[0]
    width, height = struct.unpack_from("<ii", data, 18)
    bits = struct.unpack_from("<H", data, 28)[0]
    if bits not in (24, 32):
        raise ValueError(f"{bits}-bit BMP is not supported")
    step = bits // 8
    stride = (width * step + 3) // 4 * 4
    rows = []
    for y in range(abs(height)):
        line = data[offset + y * stride : offset + y * stride + width * step]
        rgb = bytearray(width * 3)
        rgb[0::3] = line[2::step]
        rgb[1::3] = line[1::step]
        rgb[2::3] = line[0::step]
        rows.append(bytes(rgb))
    if height > 0:
        rows.reverse()
    return width, abs(height), rows


def write_bmp(width, rows):
    """A 24-bit top-down BMP from RGB rows."""
    stride = (width * 3 + 3) // 4 * 4
    body = bytearray()
    for rgb in rows:
        line = bytearray(stride)
        line[0 : width * 3 : 3] = rgb[2::3]
        line[1 : width * 3 : 3] = rgb[1::3]
        line[2 : width * 3 : 3] = rgb[0::3]
        body += line
    header = struct.pack("<2sIHHI", b"BM", 54 + len(body), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, width, -len(rows), 1, 24, 0, len(body), 0, 0, 0, 0)
    return header + info + bytes(body)


def compare(shot, baseline):
    """Returns (problems, diff_bmp_or_None, share). problems is empty when the shot matches."""
    sw, sh, shot_rows = read_bmp(shot)
    bw, bh, base_rows = read_bmp(baseline)
    if (sw, sh) != (bw, bh):
        return [f"the shot is {sw}x{sh} but the baseline is {bw}x{bh}"], None, 1.0
    differing = 0
    diff_rows = []
    for shot_row, base_row in zip(shot_rows, base_rows):
        if shot_row == base_row:
            diff_rows.append(bytes(c // 3 + 170 for c in shot_row))
            continue
        out = bytearray(len(shot_row))
        for x in range(0, len(shot_row), 3):
            pixel = shot_row[x : x + 3]
            if any(abs(a - b) > CHANNEL_LIMIT for a, b in zip(pixel, base_row[x : x + 3])):
                differing += 1
                out[x : x + 3] = b"\xff\x00\x00"
            else:
                out[x : x + 3] = bytes(c // 3 + 170 for c in pixel)
        diff_rows.append(bytes(out))
    share = differing / (sw * sh)
    problems = []
    if share > MAX_SHARE:
        problems.append(f"{share:.2%} of the pixels differ, the limit is {MAX_SHARE:.1%}")
    return problems, write_bmp(sw, diff_rows), share


def main(args):
    report_only = "--report-only" in args
    args = [arg for arg in args if arg != "--report-only"]
    if len(args) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    with open(args[0], "rb") as shot, open(args[1], "rb") as baseline:
        problems, diff, share = compare(shot.read(), baseline.read())
    if diff is not None:
        with open(args[2], "wb") as out:
            out.write(diff)
    name = args[0]
    for problem in problems:
        print(f"compare-shots: {name}: {problem}", file=sys.stderr)
    if not problems:
        print(f"compare-shots: {name}: matches ({share:.3%} differ)")
    if problems and report_only:
        print(f"compare-shots: {name}: report only, not failing")
        return 0
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
