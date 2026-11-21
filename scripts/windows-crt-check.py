#!/usr/bin/env python3
"""Fails when a Windows exe needs the Visual C++ runtime DLLs.

Usage: windows-crt-check.py <exe> [<exe> ...]

A clean Windows has no VCRUNTIME140.dll, so such an exe does not start after install. The
build links the C runtime statically (.cargo/config.toml, crt-static); this keeps it that way.
Reads the import table of each PE file itself, so it needs no Windows SDK tool.
"""

import re
import struct
import sys

# The Visual C++ runtime and the universal CRT forwarders it pulls in.
CRT = re.compile(r"^(vcruntime\d+(_\d+)?|msvcp\d+(_\w+)?|ucrtbased?|api-ms-win-crt-[\w-]+)\.dll$", re.I)


def imports(data):
    """The DLL names in the import table of a PE file (bytes)."""
    if data[:2] != b"MZ":
        raise ValueError("not a PE file (no MZ header)")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError("not a PE file (no PE signature)")
    sections, optional_size = struct.unpack_from("<H12xH", data, pe + 6)
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = {0x10B: 96, 0x20B: 112}.get(magic)
    if directories is None:
        raise ValueError(f"unknown optional header magic {magic:#x}")
    table_rva = struct.unpack_from("<I", data, optional + directories + 8)[0]
    if table_rva == 0:
        return []
    headers = [
        struct.unpack_from("<8xIIII", data, optional + optional_size + 40 * i)
        for i in range(sections)
    ]

    def offset(rva):
        for size, address, raw_size, raw in headers:
            if address <= rva < address + max(size, raw_size):
                return rva - address + raw
        raise ValueError(f"RVA {rva:#x} is in no section")

    names, at = [], offset(table_rva)
    while True:
        descriptor = struct.unpack_from("<5I", data, at)
        if descriptor == (0, 0, 0, 0, 0):
            return names
        start = offset(descriptor[3])
        names.append(data[start:data.index(b"\0", start)].decode("ascii"))
        at += 20


def main(paths):
    if not paths:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    failed = False
    for path in paths:
        with open(path, "rb") as file:
            dlls = imports(file.read())
        bad = [name for name in dlls if CRT.match(name)]
        print(f"{path}: {', '.join(dlls) or 'no imports'}")
        if bad:
            failed = True
            print(f"::error::{path} needs the Visual C++ runtime ({', '.join(bad)}); "
                  "build with crt-static (.cargo/config.toml).")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
