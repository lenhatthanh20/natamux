#!/usr/bin/env python3
"""Tests of windows-crt-check.py. Run: python3 scripts/windows-crt-check.test.py"""

import contextlib
import importlib.util
import io
import pathlib
import struct
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "windows_crt_check", pathlib.Path(__file__).with_name("windows-crt-check.py")
)
crt = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(crt)


def pe(dlls, plus=True):
    """A minimal PE file: one section at RVA 0x1000 (file offset 0x400) with the import table."""
    magic, directories = (0x20B, 112) if plus else (0x10B, 96)
    optional = bytearray(directories + 16 * 8)
    struct.pack_into("<H", optional, 0, magic)
    struct.pack_into("<I", optional, directories + 8, 0x1000 if dlls else 0)
    coff = struct.pack("<HH12xHH", 0x8664, 1, len(optional), 0x22)
    section = struct.pack("<8sIIII16x", b".idata", 0x400, 0x1000, 0x400, 0x400)
    head = bytearray(0x400)
    head[:2] = b"MZ"
    struct.pack_into("<I", head, 0x3C, 0x40)
    header = b"PE\0\0" + coff + bytes(optional) + section
    head[0x40:0x40 + len(header)] = header
    body, names = bytearray(), bytearray()
    for dll in dlls:
        body += struct.pack("<5I", 0, 0, 0, 0x1000 + 0x100 + len(names), 0)
        names += dll.encode() + b"\0"
    body += bytes(20)
    body = body.ljust(0x100, b"\0") + names
    return bytes(head) + bytes(body.ljust(0x400, b"\0"))


STATIC = ["KERNEL32.dll", "USER32.dll", "ntdll.dll", "d3d11.dll"]
DYNAMIC = STATIC + ["VCRUNTIME140.dll", "api-ms-win-crt-runtime-l1-1-0.dll"]


class ImportsTest(unittest.TestCase):
    def test_reads_the_dll_names(self):
        self.assertEqual(crt.imports(pe(DYNAMIC)), DYNAMIC)
        self.assertEqual(crt.imports(pe(STATIC, plus=False)), STATIC)

    def test_no_import_table(self):
        self.assertEqual(crt.imports(pe([])), [])

    def test_not_a_pe_file(self):
        with self.assertRaises(ValueError):
            crt.imports(b"#!/bin/sh\n" + bytes(0x100))

    def test_crt_names(self):
        for name in ("VCRUNTIME140.dll", "vcruntime140_1.dll", "MSVCP140.dll", "msvcp140_atomic_wait.dll",
                     "ucrtbase.dll", "api-ms-win-crt-heap-l1-1-0.dll"):
            self.assertTrue(crt.CRT.match(name), name)
        for name in STATIC + ["api-ms-win-core-synch-l1-2-0.dll", "msvcrt.dll"]:
            self.assertFalse(crt.CRT.match(name), name)


class MainTest(unittest.TestCase):
    def run_main(self, files):
        with tempfile.TemporaryDirectory() as folder:
            paths = []
            for index, data in enumerate(files):
                path = pathlib.Path(folder, f"{index}.exe")
                path.write_bytes(data)
                paths.append(str(path))
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = crt.main(paths)
            return code, out.getvalue()

    def test_static_build_passes(self):
        code, out = self.run_main([pe(STATIC), pe(STATIC)])
        self.assertEqual(code, 0)
        self.assertNotIn("::error::", out)

    def test_one_dynamic_exe_fails(self):
        code, out = self.run_main([pe(STATIC), pe(DYNAMIC)])
        self.assertEqual(code, 1)
        self.assertIn("VCRUNTIME140.dll", out)
        self.assertEqual(out.count("::error::"), 1)


if __name__ == "__main__":
    unittest.main()
