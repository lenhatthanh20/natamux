use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use natamux_git::gh::GitHubError;
use serde_json::{Value, json};

use super::*;

#[derive(Clone)]
struct Run {
    id: u64,
    name: &'static str,
    status: &'static str,
    conclusion: &'static str,
    url: &'static str,
}

fn run() -> Run {
    Run {
        id: 7,
        name: "CI",
        status: "completed",
        conclusion: "success",
        url: "https://github.com/o/r/actions/runs/7",
    }
}

fn running(mut each: Run) -> Run {
    each.status = "in_progress";
    each.conclusion = "";
    each
}

fn json_of(runs: &[Run]) -> String {
    let list: Vec<Value> = runs
        .iter()
        .map(|each| {
            json!({"databaseId": each.id, "workflowName": each.name, "status": each.status,
                "conclusion": each.conclusion, "url": each.url})
        })
        .collect();
    Value::Array(list).to_string()
}

#[derive(Clone)]
enum Answer {
    Runs(Vec<Run>),
    Fail(&'static str),
}

struct Fakes {
    deps: CiDeps,
    pushed: Arc<Mutex<Vec<String>>>,
    calls: Arc<Mutex<Vec<Vec<String>>>>,
    snapshots: Arc<AtomicUsize>,
}

fn fakes(answers: Vec<Answer>, logs: &str, before: Answer) -> Fakes {
    let clock = Arc::new(AtomicU64::new(0));
    let poll = Arc::new(AtomicUsize::new(0));
    let pushed = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(Vec::new()));
    let snapshots = Arc::new(AtomicUsize::new(0));
    let logs = logs.to_string();
    let (calls_in, poll_in, snapshots_in, pushed_in) = (
        calls.clone(),
        poll.clone(),
        snapshots.clone(),
        pushed.clone(),
    );
    let gh_calls = calls_in;
    let deps = CiDeps {
        snapshot: Arc::new(move |_, _| {
            snapshots_in.fetch_add(1, Ordering::SeqCst);
            Ok("abc123".into())
        }),
        push: Arc::new(move |_, sha, ref_name| {
            pushed_in.lock().unwrap().push(format!("{sha} {ref_name}"));
            Ok(())
        }),
        gh: Arc::new(move |_, args| {
            gh_calls.lock().unwrap().push(args.to_vec());
            if args[0] == "run" && args[1] == "view" {
                return Ok(logs.clone());
            }
            // The first read is made before the push; the last answer repeats.
            let at = poll_in.fetch_add(1, Ordering::SeqCst);
            let answer = if at == 0 {
                before.clone()
            } else {
                answers[(at - 1).min(answers.len() - 1)].clone()
            };
            match answer {
                Answer::Runs(runs) => Ok(json_of(&runs)),
                Answer::Fail(message) => Err(GitHubError {
                    message: message.into(),
                    output: String::new(),
                }),
            }
        }),
        sleep: {
            let clock = clock.clone();
            Arc::new(move |ms| {
                clock.fetch_add(ms, Ordering::SeqCst);
            })
        },
        now: Arc::new(move || clock.load(Ordering::SeqCst)),
    };
    Fakes {
        deps,
        pushed,
        calls,
        snapshots,
    }
}

fn simple(answers: Vec<Answer>) -> Fakes {
    fakes(answers, "Error: 2 tests failed\n", Answer::Runs(vec![]))
}

const BRANCH: &str = "natamux/t-0001-fix";

fn check(deps: &CiDeps) -> CiResult {
    check_on_ci(Path::new("/work/T-0001"), BRANCH, deps)
}

fn pushed(fakes: &Fakes) -> Vec<String> {
    fakes.pushed.lock().unwrap().clone()
}

#[test]
fn pushes_a_snapshot_to_the_ci_branch_and_passes_when_every_run_succeeds() {
    let fakes = simple(vec![
        Answer::Runs(vec![running(run())]),
        Answer::Runs(vec![run()]),
    ]);

    let result = check(&fakes.deps);

    assert_eq!(pushed(&fakes), ["abc123 natamux-ci/natamux/t-0001-fix"]);
    let first = fakes.calls.lock().unwrap()[0].clone();
    assert_eq!(
        first,
        [
            "run",
            "list",
            "--commit",
            "abc123",
            "--json",
            "databaseId,workflowName,status,conclusion,url",
            "--limit",
            "50"
        ]
    );
    assert_eq!(
        result,
        CiResult {
            passed: true,
            reasons: vec![],
            passes: vec!["CI passed: CI https://github.com/o/r/actions/runs/7".into()],
            final_: false,
            logs: None,
        }
    );
}

#[test]
fn pushes_nothing_for_a_commit_that_has_runs_already_and_waits_on_those() {
    let mut open = running(run());
    open.id = 5;
    open.name = "E2E";
    let mut done = run();
    done.id = 5;
    done.name = "E2E";
    let fakes = fakes(
        vec![Answer::Runs(vec![open.clone()]), Answer::Runs(vec![done])],
        "",
        Answer::Runs(vec![open]),
    );

    let result = check(&fakes.deps);

    assert!(pushed(&fakes).is_empty());
    assert_eq!(fakes.snapshots.load(Ordering::SeqCst), 1);
    assert!(result.passed);
    assert_eq!(
        result.passes,
        ["CI passed: E2E https://github.com/o/r/actions/runs/7"]
    );
}

#[test]
fn stops_for_the_user_when_gh_cannot_read_the_runs_before_the_push() {
    let fakes = fakes(
        vec![Answer::Runs(vec![run()])],
        "",
        Answer::Fail("gh auth login: not logged in"),
    );

    let result = check(&fakes.deps);

    assert!(!result.passed && result.final_);
    assert!(result.reasons[0].starts_with("Could not read the CI runs: "));
    assert!(pushed(&fakes).is_empty());
}

#[test]
fn pushes_when_the_first_read_fails_with_an_error_that_can_pass() {
    let fakes = fakes(
        vec![Answer::Runs(vec![run()])],
        "",
        Answer::Fail("HTTP 502"),
    );

    let result = check(&fakes.deps);

    assert_eq!(pushed(&fakes), ["abc123 natamux-ci/natamux/t-0001-fix"]);
    assert!(result.passed);
}

#[test]
fn names_each_failed_run_in_one_line_and_keeps_the_end_of_its_log_for_the_agent() {
    let mut failed = run();
    failed.id = 9;
    failed.name = "E2E";
    failed.conclusion = "failure";
    failed.url = "u9";
    let logs = format!("{}Error: 2 tests failed\n", "x".repeat(5000));
    let fakes = fakes(
        vec![Answer::Runs(vec![run(), failed])],
        &logs,
        Answer::Runs(vec![]),
    );

    let result = check(&fakes.deps);

    assert!(!result.passed);
    assert!(!result.final_);
    assert_eq!(result.reasons, ["CI E2E failure: u9"]);
    let logs = result.logs.unwrap();
    assert_eq!(logs.len(), 1);
    assert!(logs[0].starts_with("CI E2E failure: u9\n"));
    assert!(logs[0].contains("Error: 2 tests failed"));
    assert!(logs[0].chars().count() < 3200);
    assert_eq!(
        fakes.calls.lock().unwrap().last().unwrap(),
        &["run", "view", "9", "--log-failed"]
    );
}

fn failed_e2e() -> Run {
    let mut failed = run();
    failed.name = "E2E";
    failed.conclusion = "failure";
    failed.url = "u7";
    failed
}

fn with_log(runs: Vec<Run>, log: &str) -> CiResult {
    let fakes = fakes(vec![Answer::Runs(runs)], log, Answer::Runs(vec![]));
    check(&fakes.deps)
}

#[test]
fn cleans_colors_a_bom_and_carriage_returns_out_of_the_log_the_agent_gets() {
    let log = [
        "\u{feff}test\t\x1b[31mFAIL\x1b[0m a.test.ts\r\n",
        "\x1b]8;;u\x07link\x1b]8;;\x07\r",
        "next\x07\n",
    ]
    .concat();

    let result = with_log(vec![failed_e2e()], &log);

    assert_eq!(
        result.logs,
        Some(vec![
            "CI E2E failure: u7\ntest\tFAIL a.test.ts\nlink\nnext\n".to_string()
        ])
    );
}

#[test]
fn keeps_the_end_of_the_log_within_its_limit_once_the_colors_are_out() {
    let log = format!(
        "{}\x1b[31mError: 2 tests failed\x1b[0m\n",
        "\x1b[32mok\x1b[0m\r\n".repeat(1000)
    );

    let result = with_log(vec![failed_e2e()], &log);

    let kept = result.logs.unwrap().remove(0);
    assert!(kept.ends_with("Error: 2 tests failed\n"));
    assert!(!kept.contains('\x1b'));
    assert!(!kept.contains('\r'));
    assert!(kept.len() <= "CI E2E failure: u7\n".len() + 3000);
    assert!(kept.len() > 2900);
}

#[test]
fn names_every_failed_run_in_order_and_only_those() {
    let mut build = run();
    (build.id, build.name, build.conclusion, build.url) = (1, "Build", "failure", "u1");
    let mut e2e = run();
    (e2e.id, e2e.name, e2e.conclusion, e2e.url) = (2, "E2E", "cancelled", "u2");
    let mut lint = run();
    (lint.id, lint.name, lint.url) = (3, "Lint", "u3");

    let result = check(&simple(vec![Answer::Runs(vec![build, e2e, lint])]).deps);

    assert_eq!(
        result.reasons,
        ["CI Build failure: u1", "CI E2E cancelled: u2"]
    );
    let firsts: Vec<String> = result
        .logs
        .unwrap()
        .iter()
        .map(|log| log.split('\n').next().unwrap_or("").to_string())
        .collect();
    assert_eq!(firsts, result.reasons);
}

#[test]
fn keeps_one_line_for_a_failed_run_without_a_log() {
    let result = with_log(vec![failed_e2e()], "");

    assert_eq!(result.reasons, ["CI E2E failure: u7"]);
    assert_eq!(result.logs, Some(vec!["CI E2E failure: u7".to_string()]));
}

#[test]
fn stops_for_the_user_when_no_ci_run_starts_on_the_branch() {
    let result = check(&simple(vec![Answer::Runs(vec![])]).deps);

    assert!(!result.passed);
    assert!(result.final_);
    assert!(result.reasons[0].contains("natamux-ci/**"));
}

#[test]
fn stops_for_the_user_when_ci_takes_too_long_or_the_push_or_gh_fails() {
    let mut queued = run();
    queued.status = "queued";
    queued.conclusion = "";
    let slow = check(&simple(vec![Answer::Runs(vec![queued])]).deps);
    assert!(!slow.passed && slow.final_);
    assert!(slow.reasons[0].contains("30 minutes"));

    // No runs before the push, so the push is made.
    let mut no_push = simple(vec![Answer::Runs(vec![run()])]);
    no_push.deps.push = Arc::new(|_, _, _| Err("denied".into()));
    let push_failed = check(&no_push.deps);
    assert!(!push_failed.passed && push_failed.final_);
    assert!(push_failed.reasons[0].contains("denied"));

    let no_gh = check(&simple(vec![Answer::Fail("gh is not installed or not on the PATH")]).deps);
    assert!(!no_gh.passed && no_gh.final_);
}

#[test]
fn reads_again_after_a_gh_error_that_passes() {
    let result = check(&simple(vec![Answer::Fail("HTTP 502"), Answer::Runs(vec![run()])]).deps);
    assert!(result.passed);
}
