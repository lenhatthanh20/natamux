//! Ports `packages/daemon/src/stages/__tests__/stage-paths.test.ts`. The "broken" command of
//! Node's fake (it throws) is left out: `RunCommand` cannot throw, it returns a `Ran`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use natamux_proto::stages::schema::{Condition, Gate, Stage, StageKind};

use super::*;
use crate::checks::Ran;
use natamux_git::test_support::TempDir;

type Log = Arc<Mutex<Vec<(String, PathBuf)>>>;

// A shell where `true` exits 0, `false` exits 1, `slow` times out.
fn fake() -> (RunCommand, Log) {
    let log: Log = Log::default();
    let seen = log.clone();
    let command: RunCommand = Arc::new(move |command, cwd| {
        seen.lock()
            .unwrap()
            .push((command.to_string(), cwd.to_path_buf()));
        let (code, output) = match command {
            "slow" => (1, "still going\n(timed out)"),
            "missing-command" => (127, "sh: missing-command: not found\n"),
            "true" => (0, ""),
            _ => (1, ""),
        };
        Ran {
            code,
            output: output.into(),
        }
    });
    (command, log)
}

fn plan(folder: &Path, text: &str) {
    std::fs::write(folder.join("plan.md"), text).unwrap();
}

fn small() -> Condition {
    Condition::Output {
        file: "plan.md".into(),
        section: Some("Size".into()),
        contains: "small".into(),
    }
}

fn cmd(run: &str) -> Condition {
    Condition::Command { run: run.into() }
}

fn held_small(folder: &Path) -> Held {
    file_holds("plan.md", Some("Size"), "small", folder)
}

fn held_cmd(run: &str, cwd: &str, command: &RunCommand) -> Held {
    command_holds(run, Path::new(cwd), command)
}

fn held(holds: bool, because: &str, trouble: bool) -> Held {
    Held {
        holds,
        because: because.into(),
        trouble,
    }
}

fn path(id: &str, when: Option<Condition>, not: bool) -> BranchPath {
    BranchPath {
        id: id.into(),
        name: None,
        when,
        not: not.then_some(true),
        stages: vec![Stage::new(id, StageKind::Agent, Gate::Auto)],
    }
}

fn pick(folder: &Path, command: &RunCommand, paths: Vec<BranchPath>, has: &[&str]) -> PickedPath {
    let branch = StageBranch {
        id: "after-plan".into(),
        name: None,
        paths,
    };
    let input = PathInput {
        folder: folder.to_path_buf(),
        cwd: folder.join("root"),
        paths: has.iter().map(|id| id.to_string()).collect(),
    };
    pick_path(&branch, &input, command)
}

fn picked(id: Option<&str>, because: &str) -> PickedPath {
    PickedPath {
        path: id.map(str::to_string),
        because: because.into(),
    }
}

const ALL: [&str; 3] = ["a", "b", "c"];

#[test]
fn finds_the_text_under_the_section_in_any_case() {
    let dir = TempDir::new("paths");
    plan(
        dir.path(),
        "# Plan\n## Size\nA SMALL change\n## Steps\n- one\n",
    );
    assert_eq!(
        held_small(dir.path()),
        held(true, "plan.md \"Size\" contains \"small\"", false)
    );
    assert!(file_holds("plan.md", None, "small", dir.path()).holds);
}

#[test]
fn does_not_hold_for_the_text_outside_the_section() {
    let dir = TempDir::new("paths");
    plan(
        dir.path(),
        "# Plan\n## Size\nLarge\n## Steps\n- a small step\n",
    );
    assert_eq!(
        held_small(dir.path()),
        held(false, "plan.md \"Size\" does not contain \"small\"", false)
    );
}

#[test]
fn keeps_the_lines_of_a_deeper_heading_in_the_section_as_the_checks_do() {
    let dir = TempDir::new("paths");
    plan(dir.path(), "## Size\n### Why\nsmall\n## Steps\n");
    assert!(held_small(dir.path()).holds);
}

#[test]
fn does_not_hold_without_the_section_with_an_empty_one_or_without_the_file() {
    let dir = TempDir::new("paths");
    plan(dir.path(), "# Plan\nsmall\n");
    assert!(!held_small(dir.path()).holds);
    plan(dir.path(), "## Size\n## Steps\nsmall\n");
    assert!(!held_small(dir.path()).holds);
    std::fs::remove_file(dir.path().join("plan.md")).unwrap();
    assert_eq!(
        held_small(dir.path()),
        held(false, "plan.md is missing", false)
    );
}

#[test]
fn does_not_hold_for_a_file_it_cannot_read() {
    let dir = TempDir::new("paths");
    std::fs::create_dir(dir.path().join("plan.md")).unwrap();
    assert!(!held_small(dir.path()).holds);
}

#[test]
fn holds_only_when_the_command_exits_0_in_the_folder_it_is_given() {
    let (command, log) = fake();
    assert_eq!(
        held_cmd("true", "/work", &command),
        held(true, "true exited 0", false)
    );
    assert_eq!(
        *log.lock().unwrap(),
        vec![("true".to_string(), PathBuf::from("/work"))]
    );
    assert_eq!(
        held_cmd("false", "/work", &command),
        held(false, "false did not exit 0", false)
    );
}

#[test]
fn keeps_the_reason_and_the_end_of_the_output_of_a_command_that_failed() {
    let (command, _) = fake();
    assert_eq!(
        held_cmd("missing-command", "/w", &command),
        held(
            false,
            "missing-command did not exit 0:\nsh: missing-command: not found",
            false
        )
    );
    assert_eq!(
        held_cmd("slow", "/w", &command),
        held(
            false,
            "slow did not exit 0:\nstill going\n(timed out)",
            true
        )
    );
}

#[test]
fn keeps_only_the_last_2000_characters_of_the_output() {
    let long: RunCommand = Arc::new(|_, _| Ran {
        code: 2,
        output: format!("{}end", "a".repeat(3000)),
    });
    let result = held_cmd("x", "/w", &long);
    assert_eq!(
        result.because,
        format!("x did not exit 0:\n{}end", "a".repeat(1997))
    );
}

#[test]
fn takes_the_first_path_that_holds_and_runs_no_condition_after_it() {
    let dir = TempDir::new("paths");
    let (command, log) = fake();
    let result = pick(
        dir.path(),
        &command,
        vec![
            path("a", Some(cmd("true")), false),
            path("b", Some(cmd("other")), false),
        ],
        &ALL,
    );
    assert_eq!(result, picked(Some("a"), "true exited 0"));
    let ran: Vec<String> = log
        .lock()
        .unwrap()
        .iter()
        .map(|each| each.0.clone())
        .collect();
    assert_eq!(ran, vec!["true".to_string()]);
}

#[test]
fn runs_the_commands_in_the_folder_it_is_given() {
    let dir = TempDir::new("paths");
    let (command, log) = fake();
    pick(
        dir.path(),
        &command,
        vec![path("a", Some(cmd("true")), false), path("b", None, false)],
        &ALL,
    );
    assert_eq!(
        *log.lock().unwrap(),
        vec![("true".to_string(), dir.path().join("root"))]
    );
}

#[test]
fn takes_a_later_path_when_only_it_holds() {
    let dir = TempDir::new("paths");
    plan(dir.path(), "## Size\nsmall\n");
    let (command, _) = fake();
    let result = pick(
        dir.path(),
        &command,
        vec![
            path("a", Some(cmd("false")), false),
            path("b", Some(small()), false),
        ],
        &ALL,
    );
    assert_eq!(
        result,
        picked(Some("b"), "plan.md \"Size\" contains \"small\"")
    );
}

#[test]
fn takes_the_otherwise_path_when_no_condition_holds_and_skips_without_one() {
    let dir = TempDir::new("paths");
    let (command, _) = fake();
    let otherwise = pick(
        dir.path(),
        &command,
        vec![path("a", Some(cmd("false")), false), path("b", None, false)],
        &ALL,
    );
    assert_eq!(otherwise, picked(Some("b"), "Otherwise"));
    let none = pick(
        dir.path(),
        &command,
        vec![
            path("a", Some(cmd("false")), false),
            path("b", Some(small()), false),
        ],
        &ALL,
    );
    assert_eq!(none, picked(None, "No condition held"));
}

#[test]
fn flips_a_condition_with_not() {
    let dir = TempDir::new("paths");
    let (command, _) = fake();
    let by_command = pick(
        dir.path(),
        &command,
        vec![
            path("a", Some(cmd("true")), true),
            path("b", Some(cmd("false")), true),
        ],
        &ALL,
    );
    assert_eq!(by_command, picked(Some("b"), "false did not exit 0"));
    plan(dir.path(), "## Size\nlarge\n");
    let by_file = pick(
        dir.path(),
        &command,
        vec![path("a", Some(small()), true), path("b", None, false)],
        &ALL,
    );
    assert_eq!(
        by_file,
        picked(Some("a"), "plan.md \"Size\" does not contain \"small\"")
    );
}

#[test]
fn takes_a_path_with_not_when_its_file_is_missing_or_its_command_times_out() {
    let dir = TempDir::new("paths");
    let (command, _) = fake();
    let missing = pick(
        dir.path(),
        &command,
        vec![path("a", Some(small()), true), path("b", None, false)],
        &ALL,
    );
    assert_eq!(missing, picked(Some("a"), "plan.md is missing"));
    let timed_out = pick(
        dir.path(),
        &command,
        vec![path("a", Some(cmd("slow")), true), path("b", None, false)],
        &ALL,
    );
    assert_eq!(
        timed_out,
        picked(Some("a"), "slow did not exit 0:\nstill going\n(timed out)")
    );
}

#[test]
fn never_fails_a_missing_file_and_a_command_that_times_out_end_at_otherwise() {
    let dir = TempDir::new("paths");
    let (command, _) = fake();
    let result = pick(
        dir.path(),
        &command,
        vec![
            path("a", Some(small()), false),
            path("b", Some(cmd("slow")), false),
            path("c", None, false),
        ],
        &ALL,
    );
    assert_eq!(
        result,
        picked(
            Some("c"),
            "slow did not exit 0:\nstill going\n(timed out); Otherwise"
        )
    );
}

#[test]
fn passes_over_a_path_the_task_does_not_have_and_checks_the_next() {
    let dir = TempDir::new("paths");
    let (command, _) = fake();
    let both = || {
        vec![
            path("a", Some(cmd("true")), false),
            path("b", Some(cmd("true")), false),
        ]
    };
    assert_eq!(
        pick(dir.path(), &command, both(), &["b"]),
        picked(Some("b"), "a is not in this task; true exited 0")
    );
    let otherwise_gone = vec![path("a", Some(cmd("false")), false), path("b", None, false)];
    assert_eq!(
        pick(dir.path(), &command, otherwise_gone, &["a"]),
        picked(None, "b is not in this task; No condition held")
    );
    let all_gone = vec![path("a", Some(cmd("true")), false), path("b", None, false)];
    assert_eq!(
        pick(dir.path(), &command, all_gone, &[]),
        picked(
            None,
            "a is not in this task; b is not in this task; No condition held"
        )
    );
}
