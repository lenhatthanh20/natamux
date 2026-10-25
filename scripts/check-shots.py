#!/usr/bin/env python3
"""Checks the screenshots of the app's CI smoke run: the app drew its window.

Usage: check-shots.py <dark.bmp> <light.bmp>

A window that GPUI did not draw looks the same in both themes. So the light shot must be
clearly brighter than the dark one, and neither may be one flat colour.
"""

import struct
import sys

# Mean luminance, 0 to 1. One Light is near 0.95, One Dark near 0.15.
MIN_GAP = 0.15
# Spread of luminance below which a shot counts as one flat colour.
MIN_SPREAD = 0.02


def pixels(data):
    """Yields (r, g, b) of an uncompressed 24 or 32 bit BMP, a sample of every 7th pixel."""
    if data[:2] != b"BM":
        raise ValueError("not a BMP file")
    offset = struct.unpack_from("<I", data, 10)[0]
    width, height = struct.unpack_from("<ii", data, 18)
    bits = struct.unpack_from("<H", data, 28)[0]
    if bits not in (24, 32):
        raise ValueError(f"{bits}-bit BMP is not supported")
    step = bits // 8
    row = (width * step + 3) // 4 * 4
    for y in range(abs(height)):
        start = offset + y * row
        for x in range(0, width, 7):
            b, g, r = data[start + x * step : start + x * step + 3]
            yield r, g, b


def luminance_stats(data):
    values = [(0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 for r, g, b in pixels(data)]
    mean = sum(values) / len(values)
    spread = (sum((v - mean) ** 2 for v in values) / len(values)) ** 0.5
    return mean, spread


def check(dark, light):
    """Returns a list of problems, empty when the shots look right."""
    (dark_mean, dark_spread), (light_mean, light_spread) = map(luminance_stats, (dark, light))
    problems = []
    for name, spread in (("dark", dark_spread), ("light", light_spread)):
        if spread < MIN_SPREAD:
            problems.append(f"the {name} shot is one flat colour")
    if light_mean - dark_mean < MIN_GAP:
        problems.append(
            f"the light shot ({light_mean:.2f}) is not clearly brighter than the dark one ({dark_mean:.2f})"
        )
    return problems


def main(args):
    if len(args) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    with open(args[0], "rb") as dark, open(args[1], "rb") as light:
        problems = check(dark.read(), light.read())
    for problem in problems:
        print(f"check-shots: {problem}", file=sys.stderr)
    if not problems:
        print("check-shots: the app drew its window in both themes")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
