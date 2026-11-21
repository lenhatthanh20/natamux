#!/usr/bin/env python3
"""Tests of sbom.py. Run: python3 scripts/sbom.test.py"""

import importlib.util
import pathlib
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

    def test_reads_checksums_from_the_lock(self):
        text = f'[[package]]\nname = "used"\nversion = "1.0.0"\nsource = "{IO}"\nchecksum = "cd"\n\n[[package]]\nname = "ours"\nversion = "1.0.0"\n'
        self.assertEqual(sbom.lock_checksums(text), {("used", "1.0.0", IO): "cd"})


if __name__ == "__main__":
    unittest.main()
