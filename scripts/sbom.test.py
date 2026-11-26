#!/usr/bin/env python3
"""Tests of sbom.py. Run: python3 scripts/sbom.test.py"""

import importlib.util
import json
import pathlib
import tomllib
import unittest

SPEC = importlib.util.spec_from_file_location("sbom", pathlib.Path(__file__).with_name("sbom.py"))
sbom = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sbom)

IO = sbom.CRATES_IO
GIT = "git+https://github.com/owner/fork?rev=abc#abc123"


def package(name, version="1.0.0", source=IO, license="MIT"):
    return {"id": f"{name} {version}", "name": name, "version": version, "source": source, "license": license,
            "repository": f"https://example.org/{name}"}


def dep(name, kind=None, version="1.0.0"):
    return {"pkg": f"{name} {version}", "dep_kinds": [{"kind": kind}]}


METADATA = {
    "packages": [
        package("natamux-app", source=None), package("natamux-core", source=None),
        package("xtask", source=None), package("used"), package("deep"), package("builder"),
        package("dev-only"), package("xtask-only"), package("forked", source=GIT, license=None),
    ],
    "workspace_members": ["natamux-app 1.0.0", "natamux-core 1.0.0", "xtask 1.0.0"],
    "resolve": {"nodes": [
        {"id": "natamux-app 1.0.0", "deps": [dep("natamux-core"), dep("used"), dep("dev-only", "dev")]},
        {"id": "natamux-core 1.0.0", "deps": [dep("forked"), dep("builder", "build")]},
        {"id": "xtask 1.0.0", "deps": [dep("xtask-only")]},
        {"id": "used 1.0.0", "deps": [dep("deep")]},
        {"id": "deep 1.0.0", "deps": []}, {"id": "builder 1.0.0", "deps": []},
        {"id": "dev-only 1.0.0", "deps": []}, {"id": "xtask-only 1.0.0", "deps": []},
        {"id": "forked 1.0.0", "deps": []},
    ]},
}


def with_sqlite(version):
    meta = json.loads(json.dumps(METADATA))
    meta["packages"].append(package("libsqlite3-sys", version))
    for node in meta["resolve"]["nodes"]:
        if node["id"] == "used 1.0.0":
            node["deps"].append(dep("libsqlite3-sys", version=version))
    meta["resolve"]["nodes"].append({"id": f"libsqlite3-sys {version}", "deps": []})
    return meta


class SbomTest(unittest.TestCase):
    def setUp(self):
        self.out = sbom.sbom(METADATA, "9.9.9", "x86_64-unknown-linux-gnu", {("used", "1.0.0", IO): "ab" * 32})
        self.by_name = {c["name"]: c for c in self.out["components"]}

    def test_lists_normal_and_build_dependencies_only(self):
        self.assertEqual(sorted(self.by_name), ["builder", "deep", "forked", "used"])

    def test_a_crate_has_its_purl_license_and_hash(self):
        used = self.by_name["used"]
        self.assertEqual(used["purl"], "pkg:cargo/used@1.0.0")
        self.assertEqual(used["licenses"], [{"expression": "MIT"}])
        self.assertEqual(used["hashes"], [{"alg": "SHA-256", "content": "ab" * 32}])
        self.assertNotIn("hashes", self.by_name["deep"])

    def test_a_git_crate_names_its_repo_and_commit(self):
        forked = self.by_name["forked"]
        self.assertEqual(forked["purl"], "pkg:cargo/forked@1.0.0?vcs_url=git%2Bhttps://github.com/owner/fork%40abc123")
        self.assertNotIn("licenses", forked)

    def test_the_app_and_each_crate_list_what_they_depend_on(self):
        graph = {d["ref"]: d["dependsOn"] for d in self.out["dependencies"]}
        app = self.out["metadata"]["component"]["bom-ref"]
        self.assertEqual(app, "pkg:generic/natamux@9.9.9?target=x86_64-unknown-linux-gnu")
        self.assertEqual(len(graph[app]), 3)
        self.assertIn("pkg:cargo/used@1.0.0", graph[app])
        self.assertNotIn("pkg:cargo/xtask-only@1.0.0", graph)
        self.assertEqual(graph["pkg:cargo/used@1.0.0"], ["pkg:cargo/deep@1.0.0"])

    def test_the_bundled_sqlite_is_a_component(self):
        out = sbom.sbom(with_sqlite("0.39.1"), "9.9.9", "t", {})
        by_name = {c["name"]: c for c in out["components"]}
        self.assertEqual(by_name["sqlite"]["purl"], "pkg:generic/sqlite@3.54.1")
        self.assertEqual(by_name["sqlite"]["licenses"], [{"license": {"id": "blessing"}}])
        graph = {d["ref"]: d["dependsOn"] for d in out["dependencies"]}
        self.assertEqual(graph["pkg:cargo/libsqlite3-sys@0.39.1"], ["pkg:generic/sqlite@3.54.1"])
        self.assertIn("pkg:generic/sqlite@3.54.1", graph)
        self.assertNotIn("sqlite", self.by_name)

    def test_an_unknown_libsqlite3_sys_version_fails(self):
        with self.assertRaisesRegex(ValueError, "libsqlite3-sys 9.9.9"):
            sbom.sbom(with_sqlite("9.9.9"), "9.9.9", "t", {})

    def test_the_lock_libsqlite3_sys_version_is_in_the_table(self):
        lock = tomllib.loads((sbom.ROOT / "Cargo.lock").read_text())
        for p in lock["package"]:
            if p["name"] == "libsqlite3-sys":
                self.assertIn(p["version"], sbom.SQLITE)

    def test_the_rustc_version_is_in_the_metadata(self):
        out = sbom.sbom(METADATA, "9.9.9", "t", {}, "rustc 1.91.0 (abc 2026-10-01)")
        self.assertEqual(out["metadata"]["tools"]["components"], [{"type": "application", "name": "rustc", "version": "1.91.0"}])
        self.assertNotIn("tools", self.out["metadata"])

    def test_reads_checksums_from_the_lock(self):
        text = f'[[package]]\nname = "used"\nversion = "1.0.0"\nsource = "{IO}"\nchecksum = "cd"\n\n[[package]]\nname = "ours"\nversion = "1.0.0"\n'
        self.assertEqual(sbom.lock_checksums(text), {("used", "1.0.0", IO): "cd"})


if __name__ == "__main__":
    unittest.main()
