#!/usr/bin/env python3
"""Tests of ci-changes.py. Run: python3 scripts/ci-changes.test.py"""

import importlib.util
import json
import os
import pathlib
import subprocess
import tempfile
import unittest
from unittest import mock

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
    return sorted(k for k, v in ci.path_flags(paths).items()
                  if v and k in {"daemon", "terminal", "app", "client", "lock", "e2e"})


class Flags(unittest.TestCase):
    def test_terminal_change(self):
        self.assertEqual(
            true_flags(["crates/natamux-daemon/src/terminals/mod.rs"]),
            ["daemon", "e2e", "terminal"],
        )

    def test_daemon_source_is_not_terminal(self):
        self.assertEqual(true_flags(["crates/natamux-daemon/src/agents/errors.rs"]), ["daemon", "e2e"])

    def test_pty_is_daemon_and_terminal(self):
        self.assertEqual(true_flags(["crates/natamux-pty/src/lib.rs"]), ["daemon", "e2e", "terminal"])

    def test_proto_is_daemon_and_client(self):
        self.assertEqual(true_flags(["crates/natamux-proto/src/lib.rs"]), ["client", "daemon", "e2e"])

    def test_app_and_shots_scripts(self):
        self.assertEqual(true_flags(["crates/natamux-app/src/main.rs"]), ["app"])
        self.assertEqual(true_flags(["scripts/compare-shots.py"]), ["app"])
        self.assertEqual(true_flags(["scripts/bundle-rust-app.sh"]), ["app"])
        self.assertEqual(true_flags(["crates/natamux-update/src/signature.rs"]), ["app"])
        self.assertIn("app", true_flags(["crates/natamux-daemon/src/main.rs"]))

    def test_production_image_change_runs_e2e(self):
        self.assertIn("e2e", true_flags(["docker/prod.Dockerfile"]))

    def test_all_production_image_inputs_run_e2e_without_client_jobs(self):
        for path in ["packages/core/src/index.ts", "packages/web/src/main.tsx",
                     "package.json", "pnpm-lock.yaml", "pnpm-workspace.yaml",
                     "tsconfig.base.json", "docker/nginx.conf"]:
            flags = ci.path_flags([path])
            self.assertTrue(flags["e2e"], path)
            self.assertFalse(flags["client"], path)

    def test_client_and_cli(self):
        self.assertEqual(true_flags(["crates/natamux-cli/src/main.rs"]), ["client", "e2e"])

    def test_client_runs_e2e(self):
        # The image builds the CLI, and the e2e rest group runs its agent hook specs.
        self.assertEqual(true_flags(["crates/natamux-client/src/lib.rs"]), ["client", "e2e"])

    def test_lock(self):
        self.assertEqual(true_flags(["supply-chain/config.toml"]), ["lock"])
        self.assertEqual(true_flags(["deny.toml"]), ["lock"])

    def test_lock_file_can_break_any_build(self):
        self.assertEqual(len(true_flags(["Cargo.lock"])), 6)

    def test_inputs_of_the_daemon_image_jobs(self):
        for path in ["scripts/e2e.sh", "compose.e2e.yaml", "compose.prod.yaml", "e2e/tests/terminal-helpers.ts"]:
            self.assertEqual(true_flags([path]), ["client", "e2e"], path)

    def test_motion_script_is_app(self):
        self.assertEqual(true_flags(["scripts/motion-cost.sh"]), ["app"])

    def test_retired_contract_crate_has_no_job(self):
        self.assertEqual(true_flags(["crates/natamux-contract/tests/tasks.rs"]), [])

    def test_fake_agent(self):
        self.assertEqual(true_flags(["scripts/fake-agent.sh"]), ["daemon", "e2e"])

    def test_fake_gh_and_git_crate(self):
        self.assertEqual(true_flags(["scripts/fake-gh.sh"]), ["daemon", "e2e"])
        self.assertEqual(true_flags(["crates/natamux-git/src/exec.rs"]), ["daemon", "e2e"])

    def test_e2e_follows_the_daemon_and_the_images(self):
        for path in ["crates/natamux-daemon/src/main.rs", "e2e/playwright.config.ts",
                     "scripts/e2e.sh", ".github/workflows/rust.yml"]:
            self.assertIn("e2e", true_flags([path]), path)

    def test_e2e_is_off_for_retired_contract_app_and_docs_paths(self):
        for path in ["crates/natamux-contract/tests/tasks.rs",
                     "crates/natamux-app/src/main.rs", "docs/rust-phase4.md"]:
            self.assertNotIn("e2e", true_flags([path]), path)

    def test_docs_only_is_all_false(self):
        self.assertEqual(true_flags(["docs/rust-phase4.md"]), [])
        self.assertEqual(true_flags([]), [])

    def test_web_inputs_and_rust_inputs(self):
        for path in ["packages/core/src/index.ts", "packages/web/src/main.tsx",
                     "package.json", "pnpm-lock.yaml", "biome.json", ".github/workflows/ci.yml"]:
            self.assertTrue(ci.path_flags([path])["web"], path)
        for path in ["crates/natamux-app/src/main.rs", "xtask/src/main.rs",
                     "scripts/release.test.sh", ".github/workflows/rust.yml"]:
            self.assertTrue(ci.path_flags([path])["check"], path)
        self.assertFalse(ci.path_flags(["packages/web/src/main.tsx"])["check"])
        self.assertFalse(ci.path_flags(["crates/natamux-app/src/main.rs"])["web"])

    def test_check_reads_clippy_release_and_site_inputs(self):
        for path in ["clippy.toml", "package.json", "site/src/content/docs/docs/guides/terminal.mdx"]:
            self.assertTrue(ci.path_flags([path])["check"], path)

    def test_unknown_root_path_runs_every_job(self):
        for path in [".gitattributes", ".dockerignore"]:
            flags = ci.path_flags([path])
            self.assertTrue(all(flags.values()), path)
            out = ci.compute([path], "", False, GRAPH)
            self.assertEqual(out["crates"].split(), sorted(GRAPH), path)

    def test_release_note_prepares_intel_cache(self):
        self.assertTrue(ci.path_flags(["docs/releases/v1.0.17.md"])["release_prep"])
        self.assertFalse(ci.path_flags(["docs/release-process.md"])["release_prep"])

    def test_all_true_paths(self):
        for path in [".github/workflows/rust.yml", "rust-toolchain.toml", "Cargo.toml",
                     ".cargo/config.toml", "xtask/src/main.rs", "scripts/ci-changes.py",
                     "scripts/ci-wait-artifact.sh"]:
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

    def test_compute_gives_json_for_a_tag(self):
        out = ci.compute(["crates/natamux-daemon/src/main.rs"], "[e2e: timing,rest]", False, GRAPH)
        self.assertEqual(out["e2e_groups"], '["timing", "rest"]')

    def test_run_all_ignores_the_tag(self):
        out = ci.compute(["docs/x.md"], "[e2e: rest]", True, GRAPH)
        self.assertEqual(out["e2e_groups"], '["timing", "cache", "terminal", "rest"]')
        self.assertEqual(out["e2e"], "true")

    def test_main_ignores_narrow_and_invalid_e2e_tags(self):
        for tag in ["[e2e: rest]", "[e2e: no-such-group]"]:
            out = ci.compute(["packages/web/src/main.tsx"], tag, False, GRAPH,
                             "refs/heads/main")
            self.assertEqual(json.loads(out["e2e_groups"]), ci.E2E_GROUPS)
            self.assertEqual(out["e2e"], "true")


class RunAll(unittest.TestCase):
    def test_tag(self):
        self.assertTrue(ci.parse_all("Probe\n\n[ci: all]"))
        self.assertTrue(ci.parse_all("[ci:all]"))
        self.assertFalse(ci.parse_all("Probe"))
        self.assertFalse(ci.parse_all("[ci: some]"))

    def run_all(self, ref, message):
        return ci.wants_all(ref, message)

    def test_main_push_runs_all(self):
        self.assertFalse(self.run_all("refs/heads/main", "Fix"))
        self.assertTrue(self.run_all("refs/heads/main", "Fix [ci: all]"))

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
    def test_no_tag_shoots_nothing(self):
        self.assertEqual(ci.shot_shards("Fix a thing", SNAP), [])
        self.assertEqual(ci.shot_shards("", SNAP), [])

    def test_shots_line_has_one(self):
        self.assertEqual(ci.shot_shards("Snap\n\nShots: palette toast\n", SNAP), [1])

    def test_shots_or_all_tag_has_four(self):
        self.assertEqual(ci.shot_shards("Snap [ci: shots]", SNAP), [1, 2, 3, 4])
        self.assertEqual(ci.shot_shards("Snap [ci: all]", SNAP), [1, 2, 3, 4])
        self.assertEqual(ci.shot_shards("Snap [ci: shots]\n\nShots: palette\n", SNAP), [1, 2, 3, 4])

    def test_main_never_shoots(self):
        self.assertEqual(ci.shot_shards("Snap\n\nShots: palette\n", "refs/heads/main"), [])
        self.assertEqual(ci.shot_shards("Snap [ci: shots]", "refs/heads/main"), [])

    def test_shots_word_inside_a_line_is_not_a_shots_line(self):
        self.assertEqual(ci.shot_shards("More Shots: later", SNAP), [])


class ShotStates(unittest.TestCase):
    def test_states_of_the_first_shots_line(self):
        self.assertEqual(ci.shot_states("Snap\n\nShots:  palette toast \nShots: skeleton\n", SNAP), "palette toast")

    def test_no_line_or_main_gives_none(self):
        self.assertEqual(ci.shot_states("Snap [ci: shots]", SNAP), "")
        self.assertEqual(ci.shot_states("More Shots: later", SNAP), "")
        self.assertEqual(ci.shot_states("Shots: palette", "refs/heads/main"), "")

    def test_compute_gives_the_states(self):
        out = ci.compute(["docs/x.md"], "Snap\n\nShots: palette\n", False, GRAPH, SNAP)
        self.assertEqual(out["shots"], "palette")
        out = ci.compute(["docs/x.md"], "Snap", False, GRAPH, SNAP)
        self.assertEqual(out["shots"], "")


class Base(unittest.TestCase):
    def test_base_tag_names_a_commit(self):
        self.assertEqual(ci.parse_base("Probe [base: 0d67194b]"), "0d67194b")

    def test_no_or_bad_tag_means_main(self):
        self.assertEqual(ci.parse_base("Probe"), "origin/main")
        self.assertEqual(ci.parse_base("[base: dev]"), "origin/main")


class Compute(unittest.TestCase):
    def test_ci_config_pr_runs_check_without_every_rust_job(self):
        for path in [".github/workflows/rust.yml", "scripts/ci-changes.py"]:
            out = ci.compute([path], "", False, GRAPH, "refs/pull/589/merge")
            self.assertEqual(out["check"], "true", path)
            self.assertEqual(out["app"], "false", path)
            self.assertEqual(out["e2e"], "false", path)
            self.assertEqual(out["crates"], "", path)

    def test_release_skill_text_does_not_force_full_pr(self):
        out = ci.compute([".claude/skills/natamux-release/SKILL.md"], "", False, GRAPH,
                         "refs/pull/589/merge")
        self.assertEqual(out["app"], "false")
        self.assertEqual(out["e2e"], "false")

    def test_ci_config_main_push_runs_all(self):
        for path in [".github/workflows/rust.yml", "scripts/ci-changes.py"]:
            out = ci.compute([path], "", False, GRAPH, "refs/heads/main")
            self.assertEqual(out["app"], "true", path)
            self.assertEqual(out["e2e"], "true", path)
            self.assertEqual(out["crates"].split(), sorted(GRAPH), path)

    def test_release_note_selects_intel_prebuild_without_every_job(self):
        out = ci.compute(["docs/releases/v1.0.17.md"], "", False, GRAPH,
                         "refs/heads/main")
        self.assertEqual(out["release_prep"], "true")
        self.assertEqual(out["check"], "true")
        self.assertEqual(out["app"], "false")

    def test_unknown_diff_selects_web_and_check(self):
        out = ci.compute(None, "", False, GRAPH)
        self.assertEqual(out["check"], "true")
        self.assertEqual(out["web"], "true")

    def test_daemon_only_change(self):
        out = ci.compute(["crates/natamux-pty/src/lib.rs"], "", False, GRAPH)
        self.assertEqual(out["daemon_crates"], "-p natamux-pty -p natamux-daemon")
        self.assertEqual(out["client"], "false")

    def test_git_change_tests_its_daemon_dependents(self):
        out = ci.compute(["crates/natamux-git/src/changes.rs"], "", False, GRAPH)
        self.assertEqual(
            out["daemon_crates"], "-p natamux-git -p natamux-stages -p natamux-daemon"
        )
        self.assertEqual(out["daemon"], "true")

    def test_client_change_has_no_daemon_crates(self):
        out = ci.compute(["crates/natamux-cli/src/main.rs"], "", False, GRAPH)
        self.assertEqual(out["daemon_crates"], "")

    def test_run_all_ignores_the_tag(self):
        out = ci.compute(["docs/x.md"], "", True, GRAPH)
        self.assertEqual(set(out[k] for k in ["daemon", "app", "client", "lock"]), {"true"})
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
        out = ci.compute(["docs/x.md"], "Snap [ci: all]", True, GRAPH, SNAP)
        self.assertEqual(out["all"], "true")
        self.assertEqual(out["shards"], "[1, 2, 3, 4]")
        out = ci.compute(None, "Shots: palette", True, GRAPH, "refs/heads/main")
        self.assertEqual(out["shards"], "[]")

    def test_docs_only_pull_request_needs_no_job_flag(self):
        out = ci.compute(["docs/x.md"], "", False, GRAPH)
        self.assertEqual({out[k] for k in ["daemon", "terminal", "app", "client", "lock", "e2e"]}, {"false"})
        self.assertEqual(out["crates"], "")


class PushRange(unittest.TestCase):
    def test_main_push_uses_before_commit_with_two_dot_diff(self):
        before = "a" * 40
        after = "b" * 40
        with tempfile.TemporaryDirectory() as tmp:
            event_path = pathlib.Path(tmp) / "event.json"
            event_path.write_text(json.dumps({"before": before, "after": after}))
            with mock.patch.dict(os.environ, {"GITHUB_EVENT_PATH": str(event_path),
                                              "GITHUB_EVENT_NAME": "push",
                                              "GITHUB_REF": "refs/heads/main"}):
                with mock.patch.object(ci, "git", return_value=mock.Mock(stdout=after + "\n")):
                    self.assertEqual(ci.push_base(), before)
                with mock.patch.object(ci, "git", return_value=mock.Mock(stdout="docs/x.md\n")) as git:
                    self.assertEqual(ci.changed_files(before, three_dot=False), ["docs/x.md"])
                    git.assert_called_once_with("diff", "--name-only", f"{before}..HEAD")

    def test_missing_or_new_branch_base_runs_everything(self):
        with tempfile.TemporaryDirectory() as tmp:
            event_path = pathlib.Path(tmp) / "event.json"
            for event in [{"before": "0" * 40}, {"before": "bad"},
                          {"before": "a" * 40, "after": "bad"}, {}, []]:
                event_path.write_text(json.dumps(event))
                with mock.patch.dict(os.environ, {"GITHUB_EVENT_PATH": str(event_path)}):
                    self.assertIsNone(ci.push_base())
            event_path.write_text(json.dumps({"before": "a" * 40, "after": "b" * 40}))
            with mock.patch.dict(os.environ, {"GITHUB_EVENT_PATH": str(event_path)}):
                with mock.patch.object(ci, "git", return_value=mock.Mock(stdout="c" * 40)):
                    self.assertIsNone(ci.push_base())
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_PATH": "/missing/event.json"}):
            self.assertIsNone(ci.push_base())

    def test_pr_reads_merge_head_and_uses_merge_base_diff(self):
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request",
                                          "GITHUB_REF": "refs/pull/42/merge"}):
            with mock.patch.object(ci, "pr_head_message", return_value="Fix [ci: all]"):
                with mock.patch.object(ci, "changed_files") as changed:
                    with mock.patch.object(ci, "workspace_graph", return_value=GRAPH):
                        out = ci.outputs()
        self.assertEqual(out["all"], "true")
        changed.assert_not_called()

    def test_pr_without_override_uses_origin_main(self):
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request",
                                          "GITHUB_REF": "refs/pull/42/merge"}):
            with mock.patch.object(ci, "pr_head_message", return_value="Fix app"):
                with mock.patch.object(ci, "changed_files", return_value=["crates/natamux-app/src/main.rs"]) as changed:
                    with mock.patch.object(ci, "workspace_graph", return_value=GRAPH):
                        out = ci.outputs()
        changed.assert_called_once_with("origin/main")
        self.assertEqual(out["app"], "true")
        self.assertEqual(out["all"], "false")

    def test_main_missing_diff_runs_everything(self):
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "push",
                                          "GITHUB_REF": "refs/heads/main",
                                          "HEAD_MESSAGE": "Fix app"}):
            with mock.patch.object(ci, "push_base", return_value=None):
                with mock.patch.object(ci, "workspace_graph", return_value=GRAPH):
                    out = ci.outputs()
        self.assertEqual(out["check"], "true")
        self.assertEqual(out["web"], "true")
        self.assertEqual(out["release_prep"], "true")

    def test_main_push_classifies_its_own_diff(self):
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "push",
                                          "GITHUB_REF": "refs/heads/main",
                                          "HEAD_MESSAGE": "Fix app [e2e: no-such-group]"}):
            with mock.patch.object(ci, "push_base", return_value="a" * 40):
                with mock.patch.object(ci, "changed_files", return_value=["crates/natamux-app/src/main.rs"]) as changed:
                    with mock.patch.object(ci, "workspace_graph", return_value=GRAPH):
                        out = ci.outputs()
        changed.assert_called_once_with("a" * 40, three_dot=False)
        self.assertEqual(out["app"], "true")
        self.assertEqual(out["all"], "false")
        self.assertEqual(out["release_prep"], "false")
        self.assertEqual(json.loads(out["e2e_groups"]), ci.E2E_GROUPS)


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
