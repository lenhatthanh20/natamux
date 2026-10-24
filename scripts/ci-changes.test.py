#!/usr/bin/env python3
"""Tests of ci-changes.py. Run: python3 scripts/ci-changes.test.py"""

import importlib.util
import os
import pathlib
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "ci_changes", pathlib.Path(__file__).with_name("ci-changes.py")
)
ci = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ci)

GRAPH = {
    "natamux-proto": set(),
    "natamux-core": set(),
    "natamux-pty": set(),
    "natamux-screen": set(),
    "natamux-store": {"natamux-core"},
    "natamux-agents": {"natamux-core"},
    "natamux-stages": {"natamux-core"},
    "natamux-daemon": {"natamux-core", "natamux-store", "natamux-pty", "natamux-screen",
                       "natamux-agents", "natamux-stages", "natamux-proto"},
    "natamux-client": {"natamux-proto"},
    "natamux-cli": {"natamux-client"},
    "natamux-app": {"natamux-client"},
    "natamux-contract": set(),
    "xtask": set(),
}


def true_flags(paths):
    return sorted(k for k, v in ci.path_flags(paths).items() if v)


class Flags(unittest.TestCase):
    def test_terminal_change(self):
        self.assertEqual(
            true_flags(["crates/natamux-daemon/src/terminals/mod.rs"]),
            ["contract", "daemon", "terminal"],
        )

    def test_daemon_source_is_not_terminal(self):
        self.assertEqual(true_flags(["crates/natamux-daemon/src/agents/errors.rs"]), ["contract", "daemon"])

    def test_pty_is_daemon_and_terminal(self):
        self.assertEqual(true_flags(["crates/natamux-pty/src/lib.rs"]), ["contract", "daemon", "terminal"])

    def test_proto_is_daemon_and_client(self):
        self.assertEqual(true_flags(["crates/natamux-proto/src/lib.rs"]), ["client", "contract", "daemon"])

    def test_app_and_shots_scripts(self):
        self.assertEqual(true_flags(["crates/natamux-app/src/main.rs"]), ["app"])
        self.assertEqual(true_flags(["scripts/compare-shots.py"]), ["app"])
        self.assertEqual(true_flags(["scripts/bundle-rust-app.sh"]), ["app"])

    def test_client_and_cli(self):
        self.assertEqual(true_flags(["crates/natamux-cli/src/main.rs"]), ["client"])
        self.assertEqual(true_flags(["scripts/cli-parity.sh"]), ["client"])

    def test_lock(self):
        self.assertEqual(true_flags(["Cargo.lock"]), ["lock"])
        self.assertEqual(true_flags(["supply-chain/config.toml"]), ["lock"])

    def test_contract_crate_only(self):
        self.assertEqual(true_flags(["crates/natamux-contract/tests/tasks.rs"]), ["contract"])

    def test_fake_agent_and_compose(self):
        self.assertEqual(true_flags(["scripts/fake-agent.sh"]), ["contract", "daemon"])
        self.assertEqual(true_flags(["compose.rust-daemon.yaml"]), ["contract", "daemon"])

    def test_docs_only_is_all_false(self):
        self.assertEqual(true_flags(["docs/rust-phase4.md"]), [])
        self.assertEqual(true_flags([]), [])

    def test_all_true_paths(self):
        for path in [".github/workflows/rust.yml", "rust-toolchain.toml", "Cargo.toml",
                     ".cargo/config.toml", "xtask/src/main.rs", "scripts/ci-changes.py"]:
            self.assertEqual(len(true_flags([path])), 6, path)

    def test_failed_diff_is_all_true(self):
        self.assertEqual(len(true_flags(None)), 6)


class Closure(unittest.TestCase):
    def crates(self, *paths):
        return ci.changed_crates(list(paths), GRAPH)

    def test_core_reaches_its_dependents(self):
        out = self.crates("crates/natamux-core/src/lib.rs")
        for name in ["natamux-stages", "natamux-daemon", "natamux-agents", "natamux-store"]:
            self.assertIn(name, out)
        self.assertNotIn("natamux-cli", out)

    def test_pty_reaches_daemon_only(self):
        self.assertEqual(self.crates("crates/natamux-pty/src/lib.rs"), ["natamux-daemon", "natamux-pty"])

    def test_client_reaches_cli_and_app(self):
        self.assertEqual(
            self.crates("crates/natamux-client/src/lib.rs"),
            ["natamux-app", "natamux-cli", "natamux-client"],
        )

    def test_xtask_and_docs(self):
        self.assertEqual(self.crates("xtask/src/main.rs"), ["xtask"])
        self.assertEqual(self.crates("docs/x.md"), [])

    def test_graph_reads_all_dependency_tables(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for name, body in {
                "natamux-a": "",
                "natamux-b": '[dependencies]\nnatamux-a = { path = "../natamux-a" }\n',
                "natamux-c": '[dev-dependencies]\nnatamux-b = { workspace = true }\n',
                "natamux-d": '[target.\'cfg(unix)\'.build-dependencies]\nnatamux-c = { path = "../x" }\nserde = "1"\n',
            }.items():
                (root / "crates" / name).mkdir(parents=True)
                (root / "crates" / name / "Cargo.toml").write_text(f'[package]\nname = "{name}"\n{body}')
            graph = ci.workspace_graph(root)
        self.assertEqual(graph["natamux-b"], {"natamux-a"})
        self.assertEqual(graph["natamux-c"], {"natamux-b"})
        self.assertEqual(graph["natamux-d"], {"natamux-c"})


class Suites(unittest.TestCase):
    def test_no_tag_is_all(self):
        self.assertEqual(ci.parse_suites("Fix a thing"), ci.SUITES)

    def test_one_and_two_names(self):
        self.assertEqual(ci.parse_suites("x [contract: hooks]"), ["hooks"])
        self.assertEqual(ci.parse_suites("x [contract: hooks, stages]"), ["stages", "hooks"])

    def test_spaces(self):
        self.assertEqual(ci.parse_suites("[contract:  tasks ,  terminals ]"), ["terminals", "tasks"])

    def test_unknown_name_names_the_valid_ones(self):
        with self.assertRaises(ValueError) as caught:
            ci.parse_suites("[contract: hooks,nope]")
        self.assertIn("nope", str(caught.exception))
        self.assertIn("terminals, tasks, stages, hooks", str(caught.exception))


class Base(unittest.TestCase):
    def test_base_tag_names_a_commit(self):
        self.assertEqual(ci.parse_base("Probe [base: 0d67194b]"), "0d67194b")

    def test_no_or_bad_tag_means_dev(self):
        self.assertEqual(ci.parse_base("Probe"), "origin/dev")
        self.assertEqual(ci.parse_base("[base: dev]"), "origin/dev")


class Compute(unittest.TestCase):
    def test_daemon_only_change(self):
        out = ci.compute(["crates/natamux-pty/src/lib.rs"], "[contract: terminals]", False, GRAPH)
        self.assertEqual(out["daemon_crates"], "-p natamux-pty -p natamux-daemon")
        self.assertEqual(out["suites"], "terminals")
        self.assertEqual(out["client"], "false")

    def test_client_change_has_no_daemon_crates(self):
        out = ci.compute(["crates/natamux-cli/src/main.rs"], "", False, GRAPH)
        self.assertEqual(out["daemon_crates"], "")

    def test_run_all_ignores_the_tag(self):
        out = ci.compute(["docs/x.md"], "[contract: nope]", True, GRAPH)
        self.assertEqual(set(out[k] for k in ["daemon", "app", "client", "lock", "contract"]), {"true"})
        self.assertEqual(out["suites"], "terminals,tasks,stages,hooks")
        self.assertEqual(out["crates"].split(), sorted(GRAPH))

    def test_workflow_change_is_all_crates(self):
        out = ci.compute([".github/workflows/rust.yml"], "", False, GRAPH)
        self.assertEqual(out["crates"].split(), sorted(GRAPH))

    def test_failed_diff(self):
        out = ci.compute(None, "", False, GRAPH)
        self.assertEqual(out["app"], "true")


def git(root, *args):
    subprocess.run(
        ["git", "-c", "user.name=t", "-c", "user.email=t@t", "-C", str(root), *args],
        check=True, capture_output=True,
    )


class Mtimes(unittest.TestCase):
    def repo(self, tmp):
        root = pathlib.Path(tmp)
        git(root, "init", "-q")
        (root / "a.txt").write_text("1")
        (root / "b.txt").write_text("1")
        git(root, "add", ".")
        git(root, "commit", "-q", "-m", "one")
        first = subprocess.run(["git", "-C", str(root), "rev-parse", "HEAD"],
                               capture_output=True, text=True).stdout.strip()
        (root / "a.txt").write_text("2")
        git(root, "commit", "-q", "-am", "two")
        return root, first

    def old(self, root, name):
        return int((root / name).stat().st_mtime) == ci.OLD_TIME

    def test_only_changed_files_are_new(self):
        with tempfile.TemporaryDirectory() as tmp:
            root, first = self.repo(tmp)
            (root / "target").mkdir()
            (root / ci.STAMP).write_text(first)
            ci.set_mtimes(root)
            self.assertFalse(self.old(root, "a.txt"))
            self.assertTrue(self.old(root, "b.txt"))

    def test_unknown_or_missing_stamp_touches_everything(self):
        with tempfile.TemporaryDirectory() as tmp:
            root, _ = self.repo(tmp)
            ci.set_mtimes(root)
            self.assertFalse(self.old(root, "b.txt"))
            (root / "target").mkdir()
            (root / ci.STAMP).write_text("0" * 40)
            os.utime(root / "b.txt", (ci.OLD_TIME, ci.OLD_TIME))
            ci.set_mtimes(root)
            self.assertFalse(self.old(root, "b.txt"))

    def test_stamp_writes_head(self):
        with tempfile.TemporaryDirectory() as tmp:
            root, first = self.repo(tmp)
            ci.stamp_head(root)
            self.assertNotEqual((root / ci.STAMP).read_text().strip(), first)
            self.assertEqual(len((root / ci.STAMP).read_text().strip()), 40)


if __name__ == "__main__":
    unittest.main()
