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
    "natamux-git": {"natamux-core", "natamux-proto"},
    "natamux-stages": {"natamux-core", "natamux-git"},
    "natamux-daemon": {"natamux-core", "natamux-store", "natamux-pty", "natamux-screen",
                       "natamux-agents", "natamux-git", "natamux-stages", "natamux-proto"},
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
            ["contract", "daemon", "e2e", "terminal"],
        )

    def test_daemon_source_is_not_terminal(self):
        self.assertEqual(true_flags(["crates/natamux-daemon/src/agents/errors.rs"]), ["contract", "daemon", "e2e"])

    def test_pty_is_daemon_and_terminal(self):
        self.assertEqual(true_flags(["crates/natamux-pty/src/lib.rs"]), ["contract", "daemon", "e2e", "terminal"])

    def test_proto_is_daemon_and_client(self):
        self.assertEqual(true_flags(["crates/natamux-proto/src/lib.rs"]), ["client", "contract", "daemon", "e2e"])

    def test_app_and_shots_scripts(self):
        self.assertEqual(true_flags(["crates/natamux-app/src/main.rs"]), ["app"])
        self.assertEqual(true_flags(["scripts/compare-shots.py"]), ["app"])
        self.assertEqual(true_flags(["scripts/bundle-rust-app.sh"]), ["app"])

    def test_client_and_cli(self):
        self.assertEqual(true_flags(["crates/natamux-cli/src/main.rs"]), ["client"])
        self.assertEqual(true_flags(["scripts/cli-parity.sh"]), ["client"])

    def test_lock(self):
        self.assertEqual(true_flags(["supply-chain/config.toml"]), ["lock"])
        self.assertEqual(true_flags(["deny.toml"]), ["lock"])

    def test_lock_file_can_break_any_build(self):
        self.assertEqual(len(true_flags(["Cargo.lock"])), 7)

    def test_inputs_of_the_daemon_image_jobs(self):
        for path in ["scripts/e2e.sh", "compose.e2e.yaml", "compose.prod.yaml", "e2e/tests/terminal-helpers.ts"]:
            self.assertEqual(true_flags([path]), ["client", "contract", "e2e"], path)

    def test_inputs_of_the_contract_job(self):
        self.assertEqual(true_flags(["scripts/task-file-golden.mjs"]), ["contract"])
        self.assertEqual(true_flags(["scripts/xterm-grid.mjs"]), ["contract", "daemon"])

    def test_motion_script_is_app(self):
        self.assertEqual(true_flags(["scripts/motion-cost.sh"]), ["app"])

    def test_contract_crate_only(self):
        self.assertEqual(true_flags(["crates/natamux-contract/tests/tasks.rs"]), ["contract"])

    def test_fake_agent_and_compose(self):
        self.assertEqual(true_flags(["scripts/fake-agent.sh"]), ["contract", "daemon", "e2e"])
        self.assertEqual(true_flags(["compose.rust-daemon.yaml"]), ["contract", "daemon"])

    def test_fake_gh_and_git_crate(self):
        self.assertEqual(true_flags(["scripts/fake-gh.sh"]), ["contract", "daemon", "e2e"])
        self.assertEqual(true_flags(["crates/natamux-git/src/exec.rs"]), ["contract", "daemon", "e2e"])

    def test_e2e_follows_the_daemon_and_the_images(self):
        for path in ["crates/natamux-daemon/src/main.rs", "compose.rust-e2e.yaml", "e2e/playwright.config.ts",
                     "scripts/e2e.sh", ".github/workflows/rust.yml"]:
            self.assertIn("e2e", true_flags([path]), path)

    def test_the_artifact_wait_runs_the_jobs_that_use_it(self):
        self.assertEqual(true_flags(["scripts/ci-wait-artifact.sh"]), ["contract", "e2e"])

    def test_e2e_is_off_for_contract_only_app_and_docs_paths(self):
        for path in ["crates/natamux-contract/tests/tasks.rs", "scripts/task-file-golden.mjs",
                     "compose.rust-daemon.yaml", "scripts/xterm-grid.mjs",
                     "crates/natamux-app/src/main.rs", "docs/rust-phase4.md"]:
            self.assertNotIn("e2e", true_flags([path]), path)

    def test_docs_only_is_all_false(self):
        self.assertEqual(true_flags(["docs/rust-phase4.md"]), [])
        self.assertEqual(true_flags([]), [])

    def test_all_true_paths(self):
        for path in [".github/workflows/rust.yml", "rust-toolchain.toml", "Cargo.toml",
                     ".cargo/config.toml", "xtask/src/main.rs", "scripts/ci-changes.py",
                     ".github/workflows/rust-cache.yml"]:
            self.assertEqual(len(true_flags([path])), 7, path)

    def test_failed_diff_is_all_true(self):
        self.assertEqual(len(true_flags(None)), 7)


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
        self.assertIn("terminals, tasks, stages, hooks, workspaces", str(caught.exception))


class E2eGroups(unittest.TestCase):
    def test_no_tag_is_all(self):
        self.assertEqual(ci.parse_e2e_groups("Fix a thing"), ["timing", "cache", "terminal", "rest"])

    def test_one_and_two_names_in_canonical_order(self):
        self.assertEqual(ci.parse_e2e_groups("x [e2e: rest]"), ["rest"])
        self.assertEqual(ci.parse_e2e_groups("x [e2e: rest, timing]"), ["timing", "rest"])

    def test_unknown_name_names_the_valid_ones(self):
        with self.assertRaises(ValueError) as caught:
            ci.parse_e2e_groups("[e2e: timing,nope]")
        self.assertIn("nope", str(caught.exception))
        self.assertIn("timing, cache, terminal, rest", str(caught.exception))

    def test_projects_is_a_contract_suite(self):
        self.assertEqual(ci.parse_suites("[contract: projects]"), ["projects"])

    def test_compute_gives_json_for_a_tag(self):
        out = ci.compute(["crates/natamux-daemon/src/main.rs"], "[e2e: timing,rest]", False, GRAPH)
        self.assertEqual(out["e2e_groups"], '["timing", "rest"]')

    def test_run_all_ignores_the_tag(self):
        out = ci.compute(["docs/x.md"], "[e2e: rest]", True, GRAPH)
        self.assertEqual(out["e2e_groups"], '["timing", "cache", "terminal", "rest"]')
        self.assertEqual(out["e2e"], "true")


class RunAll(unittest.TestCase):
    def test_tag(self):
        self.assertTrue(ci.parse_all("Probe\n\n[ci: all]"))
        self.assertTrue(ci.parse_all("[ci:all]"))
        self.assertFalse(ci.parse_all("Probe"))
        self.assertFalse(ci.parse_all("[ci: some]"))

    def run_all(self, ref, message):
        return ci.wants_all(ref, message)

    def test_dev_push_runs_all(self):
        self.assertTrue(self.run_all("refs/heads/dev", "Fix"))

    def test_pull_request_reads_the_diff(self):
        self.assertFalse(self.run_all("refs/pull/7/merge", ""))
        self.assertFalse(self.run_all("refs/pull/7/merge", "Fix"))

    def test_pull_request_runs_all_with_the_tag(self):
        self.assertTrue(self.run_all("refs/pull/7/merge", "Fix [ci: all]"))

    def test_snapshot_runs_all_only_with_the_tag(self):
        self.assertFalse(self.run_all("refs/heads/natamux-ci/rust", "Snap"))
        self.assertTrue(self.run_all("refs/heads/natamux-ci/rust", "Snap [ci: all]"))


SNAP = "refs/heads/natamux-ci/rust"


class Shards(unittest.TestCase):
    def test_full_run_has_four(self):
        self.assertEqual(ci.shot_shards("Fix a thing", SNAP), [1, 2, 3, 4])
        self.assertEqual(ci.shot_shards("", SNAP), [1, 2, 3, 4])

    def test_shots_line_has_one(self):
        self.assertEqual(ci.shot_shards("Snap\n\nShots: palette toast\n", SNAP), [1])

    def test_dev_always_has_four(self):
        self.assertEqual(ci.shot_shards("Snap\n\nShots: palette\n", "refs/heads/dev"), [1, 2, 3, 4])

    def test_shots_word_inside_a_line_is_not_a_shots_line(self):
        self.assertEqual(ci.shot_shards("More Shots: later", SNAP), [1, 2, 3, 4])


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

    def test_git_change_tests_its_daemon_dependents(self):
        out = ci.compute(["crates/natamux-git/src/changes.rs"], "[contract: workspaces]", False, GRAPH)
        self.assertEqual(
            out["daemon_crates"], "-p natamux-git -p natamux-stages -p natamux-daemon"
        )
        self.assertEqual(out["suites"], "workspaces")
        self.assertEqual(out["contract"], "true")

    def test_client_change_has_no_daemon_crates(self):
        out = ci.compute(["crates/natamux-cli/src/main.rs"], "", False, GRAPH)
        self.assertEqual(out["daemon_crates"], "")

    def test_run_all_ignores_the_tag(self):
        out = ci.compute(["docs/x.md"], "[contract: nope]", True, GRAPH)
        self.assertEqual(set(out[k] for k in ["daemon", "app", "client", "lock", "contract"]), {"true"})
        self.assertEqual(out["suites"], "terminals,tasks,stages,hooks,workspaces,projects")
        self.assertEqual(out["crates"].split(), sorted(GRAPH))

    def test_workflow_change_is_all_crates(self):
        out = ci.compute([".github/workflows/rust.yml"], "", False, GRAPH)
        self.assertEqual(out["crates"].split(), sorted(GRAPH))

    def test_lock_change_is_all_crates(self):
        out = ci.compute(["Cargo.lock"], "", False, GRAPH)
        self.assertEqual(out["crates"].split(), sorted(GRAPH))

    def test_failed_diff(self):
        out = ci.compute(None, "", False, GRAPH)
        self.assertEqual(out["app"], "true")

    def test_all_and_shards_outputs(self):
        out = ci.compute(["docs/x.md"], "Shots: palette", False, GRAPH)
        self.assertEqual(out["all"], "false")
        self.assertEqual(out["shards"], "[1]")
        out = ci.compute(["docs/x.md"], "", True, GRAPH)
        self.assertEqual(out["all"], "true")
        self.assertEqual(out["shards"], "[1, 2, 3, 4]")
        out = ci.compute(None, "Shots: palette", True, GRAPH, "refs/heads/dev")
        self.assertEqual(out["shards"], "[1, 2, 3, 4]")

    def test_docs_only_pull_request_needs_no_job_flag(self):
        out = ci.compute(["docs/x.md"], "", False, GRAPH)
        self.assertEqual({out[k] for k in ["daemon", "terminal", "app", "client", "lock", "contract", "e2e"]}, {"false"})
        self.assertEqual(out["crates"], "")


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
