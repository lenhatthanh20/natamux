//! `describe('a check on CI')` of `stage-runner.test.ts`.

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use natamux_daemon::stages::act::StageActionInput;
use natamux_daemon::stages::shells::LAUNCH_GRACE_MS;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::{MAX_AGENT_SEND_BACKS, StageAction};
use natamux_stages::checks::Ran;
use natamux_stages::ci::CiResult;
use serde_json::{Value, json};

use crate::{Fixture, expect_submitted, term_input, wait_for};

fn fixture() -> Fixture {
    let f = Fixture::new();
    f.set_stages(json!([{ "id": "implement", "kind": "agent", "gate": "manual", "ci": true }]));
    f
}

fn reported(f: &Fixture, ci: bool) -> Task {
    let task = f.implementing("implement");
    if ci {
        f.set_ci(&task.id, "implement", true);
    }
    f.runner
        .run
        .act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    task
}

fn report(f: &Fixture, id: &str) {
    f.runner
        .run
        .act(id, StageAction::Report, &term_input("1"))
        .unwrap();
}

fn wait_for_calls(f: &Fixture, count: usize) {
    wait_for(|| f.ci.calls.lock().unwrap().len() == count);
}

fn payload_of(f: &Fixture, id: &str, kind: &str) -> serde_json::Map<String, Value> {
    f.last_of(id, kind).unwrap().payload
}

fn failure() -> CiResult {
    CiResult {
        passed: false,
        reasons: vec!["CI E2E failure: u".into()],
        logs: Some(vec![
            "CI E2E failure: u\n\x1b[31mError: 2 tests failed\x1b[0m".into(),
        ]),
        passes: vec![],
        final_: false,
    }
}

fn passed(passes: &[&str]) -> CiResult {
    CiResult {
        passed: true,
        passes: passes.iter().map(|text| text.to_string()).collect(),
        ..CiResult::default()
    }
}

fn state_of(f: &Fixture, id: &str) -> (String, TaskState) {
    let task = f.task(id);
    (task.stage, task.state)
}

fn running_implement() -> (String, TaskState) {
    ("implement".into(), TaskState::Running)
}

// The paste that holds `text`, and the Enter that went apart after it.
fn expect_prompt_submitted(f: &Fixture, text: &str) -> String {
    let find = || {
        let written = f.pty(0).written();
        let at = written
            .iter()
            .position(|data| data.starts_with("\x1b[200~") && data.contains(text))?;
        (written.get(at + 1).map(String::as_str) == Some("\r")).then(|| written[at].clone())
    };
    wait_for(|| find().is_some());
    find().unwrap()
}

fn prompts(f: &Fixture) -> usize {
    f.pty(0)
        .written()
        .iter()
        .filter(|data| data.contains("What to change"))
        .count()
}

fn count_of(f: &Fixture, id: &str, kind: &str) -> usize {
    f.task_events(id)
        .iter()
        .filter(|event| event.kind == kind)
        .count()
}

#[test]
fn waits_for_ci_after_the_local_checks_then_passes_the_stage() {
    let f = fixture();
    let task = reported(&f, true);

    wait_for_calls(&f, 1);
    let calls = f.ci.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![(
            task.workspace.clone().unwrap().into(),
            task.branch.clone().unwrap()
        )]
    );
    let started = payload_of(&f, &task.id, "stage.ci-started");
    assert_eq!(started["stage"], "implement");
    assert_eq!(
        started["branch"],
        format!("natamux-ci/{}", task.branch.unwrap())
    );
    assert_eq!(f.task(&task.id).state, TaskState::Running);

    f.ci.finish(passed(&["CI passed: CI u"]));
    f.settle();

    assert_eq!(f.task(&task.id).state, TaskState::Review);
    let passes = payload_of(&f, &task.id, "stage.checked")["passes"].clone();
    assert!(
        passes
            .as_array()
            .unwrap()
            .contains(&json!("CI passed: CI u"))
    );
}

#[test]
fn shows_the_ci_round_as_a_background_job_until_ci_answers() {
    let f = fixture();
    let task = reported(&f, true);

    let ci_job = |f: &Fixture| {
        f.bus.deliver_all();
        f.job_lists()
            .last()
            .and_then(|list| list.iter().find(|job| job.kind == "ci").cloned())
    };
    wait_for(|| ci_job(&f).is_some());
    let job = ci_job(&f).unwrap();
    assert_eq!(job.label, "Waiting for CI on Implement");
    assert_eq!(job.task_id.as_deref(), Some(task.id.as_str()));
    f.ci.finish(passed(&[]));
    f.settle();
    wait_for(|| {
        f.bus.deliver_all();
        f.job_lists().last().is_some_and(Vec::is_empty)
    });
}

#[test]
fn sends_the_stage_back_to_its_agent_with_the_log_as_a_review_does() {
    let f = fixture();
    let task = reported(&f, true);

    f.ci.finish(failure());
    f.settle();

    assert_eq!(
        Value::Object(payload_of(&f, &task.id, "stage.check-failed")),
        json!({ "stage": "implement", "reasons": ["CI E2E failure: u"], "final": false })
    );
    let reopened = payload_of(&f, &task.id, "stage.reopened");
    assert_eq!(reopened["to"], "implement");
    assert_eq!(reopened["from"], "implement");
    assert_eq!(reopened["byAgent"], true);
    assert_eq!(reopened["ci"], true);
    let reason = reopened["reason"].as_str().unwrap();
    assert!(
        reason.starts_with("CI failed on Implement:\n- CI E2E failure: u\n"),
        "{reason}"
    );
    assert!(reason.contains("gh run view"));
    assert_eq!(state_of(&f, &task.id), running_implement());
    assert_eq!(count_of(&f, &task.id, "stage.started"), 2);

    let paste = expect_prompt_submitted(&f, "What to change: CI failed on Implement:");
    assert!(paste.contains("Error: 2 tests failed"));
    assert!(!paste.contains("\x1b[31m"));
    assert!(
        !f.pty(0)
            .written()
            .concat()
            .contains("The checks of this stage failed")
    );
    assert_eq!(prompts(&f), 1);
}

#[test]
fn names_each_failed_run_keeps_a_run_without_a_log_and_uses_the_name_of_the_stage() {
    let f = Fixture::new();
    f.set_stages(json!([
        { "id": "implement", "name": "Build it", "kind": "agent", "gate": "manual", "ci": true },
    ]));
    let task = reported(&f, true);

    f.ci.finish(CiResult {
        reasons: vec!["CI E2E failure: u1".into(), "CI Lint failure: u2".into()],
        logs: Some(vec![
            "CI E2E failure: u1\n\x1b[31mError: 2 tests failed\x1b[0m\r\n".into(),
            "CI Lint failure: u2".into(),
        ]),
        ..failure()
    });
    f.settle();

    let expected = [
        "CI failed on Build it:",
        "- CI E2E failure: u1",
        "- CI Lint failure: u2",
        "",
        "CI E2E failure: u1\nError: 2 tests failed",
        "CI Lint failure: u2",
        "Read the failed log with `gh run view <run id> --log-failed`; the id ends the run url.",
    ]
    .join("\n");
    assert_eq!(
        payload_of(&f, &task.id, "stage.reopened")["reason"],
        expected
    );
    expect_prompt_submitted(&f, "What to change: CI failed on Build it:");
}

#[test]
fn starts_the_agent_again_with_the_failure_when_it_ended_meanwhile() {
    let f = fixture();
    let task = reported(&f, true);
    f.advance(LAUNCH_GRACE_MS + 1);

    f.ci.finish(failure());
    f.settle();

    assert_eq!(state_of(&f, &task.id), running_implement());
    let typed = f.pty(0).written().concat();
    assert_eq!(
        typed
            .matches(&format!("agent-launch-{}.sh", task.id))
            .count(),
        2
    );
    assert!(
        f.launch_script(&task.id, "1")
            .contains("What to change: CI failed on Implement:")
    );
    assert!(!typed.contains("What to change"));
}

#[test]
fn does_not_count_send_backs_by_a_person_toward_the_most_times() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_ci(&task.id, "implement", true);
    for _ in 0..MAX_AGENT_SEND_BACKS {
        let input = StageActionInput {
            to: Some("implement".into()),
            reason: Some("Name it better".into()),
            ..Default::default()
        };
        f.runner
            .run
            .act(&task.id, StageAction::Reopen, &input)
            .unwrap();
        f.settle();
    }
    report(&f, &task.id);

    f.ci.finish(failure());
    f.settle();

    assert_eq!(payload_of(&f, &task.id, "stage.reopened")["ci"], true);
    assert_eq!(state_of(&f, &task.id), running_implement());
}

#[test]
fn keeps_one_more_try_for_failed_local_checks_then_sends_a_ci_failure_back() {
    let f = Fixture::new();
    f.set_stages(json!([
        { "id": "implement", "kind": "agent", "gate": "manual", "ci": true, "check": "make test" },
    ]));
    let code = Arc::new(AtomicI32::new(1));
    let read = Arc::clone(&code);
    f.set_run_command(move |_, _| Ran {
        code: read.load(Ordering::SeqCst),
        output: "lint failed".into(),
    });
    let task = reported(&f, true);

    assert!(f.ci.calls.lock().unwrap().is_empty());
    assert_eq!(f.task(&task.id).state, TaskState::Running);
    expect_submitted(&f.pty(0), "The checks of this stage failed");
    assert!(f.last_of(&task.id, "stage.reopened").is_none());

    code.store(0, Ordering::SeqCst);
    report(&f, &task.id);
    wait_for_calls(&f, 1);
    f.ci.finish(failure());
    f.settle();

    assert_eq!(payload_of(&f, &task.id, "stage.reopened")["ci"], true);
    assert_eq!(state_of(&f, &task.id), running_implement());
}

#[test]
fn waits_for_you_after_ci_sent_the_stage_back_the_most_times() {
    let f = fixture();
    let task = reported(&f, true);
    for _ in 0..MAX_AGENT_SEND_BACKS {
        f.ci.finish(failure());
        f.settle();
        assert_eq!(state_of(&f, &task.id), running_implement());
        report(&f, &task.id);
    }

    f.ci.finish(failure());
    f.settle();

    assert_eq!(
        state_of(&f, &task.id),
        ("implement".into(), TaskState::NeedsInput)
    );
    assert_eq!(
        Value::Object(payload_of(&f, &task.id, "stage.check-failed")),
        json!({ "stage": "implement", "reasons": ["CI E2E failure: u"], "final": true })
    );
    assert_eq!(
        count_of(&f, &task.id, "stage.reopened"),
        MAX_AGENT_SEND_BACKS
    );
    // A paste waits for the Enter of the one before it.
    wait_for(|| prompts(&f) == MAX_AGENT_SEND_BACKS);
}

#[test]
fn sends_ci_failures_back_again_after_a_person_sent_the_stage_back() {
    let f = fixture();
    let task = reported(&f, true);
    for _ in 0..MAX_AGENT_SEND_BACKS {
        f.ci.finish(failure());
        f.settle();
        report(&f, &task.id);
    }
    f.ci.finish(failure());
    f.settle();
    assert_eq!(
        state_of(&f, &task.id),
        ("implement".into(), TaskState::NeedsInput)
    );

    let person = StageActionInput {
        to: Some("implement".into()),
        ..Default::default()
    };
    f.runner
        .run
        .act(&task.id, StageAction::Reopen, &person)
        .unwrap();
    f.settle();
    report(&f, &task.id);
    f.ci.finish(failure());
    f.settle();

    assert_eq!(state_of(&f, &task.id), running_implement());
    assert_eq!(payload_of(&f, &task.id, "stage.reopened")["ci"], true);
    assert_eq!(
        count_of(&f, &task.id, "stage.reopened"),
        MAX_AGENT_SEND_BACKS + 2
    );
}

#[test]
fn waits_for_you_at_once_when_no_run_started() {
    waits_at_once("No CI run started for natamux-ci/x.");
}

#[test]
fn waits_for_you_at_once_when_ci_took_too_long() {
    waits_at_once("CI did not finish within 30 minutes: u");
}

fn waits_at_once(reason: &str) {
    let f = fixture();
    let task = reported(&f, true);

    f.ci.finish(CiResult {
        passed: false,
        reasons: vec![reason.into()],
        final_: true,
        ..CiResult::default()
    });
    f.settle();

    assert_eq!(
        payload_of(&f, &task.id, "stage.check-failed")["final"],
        true
    );
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    assert!(f.last_of(&task.id, "stage.reopened").is_none());
    assert_eq!(count_of(&f, &task.id, "stage.started"), 1);
    assert!(!f.pty(0).written().concat().contains(reason));
}

#[test]
fn goes_on_with_a_ci_check_after_a_restart_and_runs_one_at_a_time() {
    let mut f = fixture();
    let task = reported(&f, true);
    report(&f, &task.id);
    wait_for_calls(&f, 1);

    f.runner.stop();
    f.runner = f.make_runner(None);
    f.runner.start();
    wait_for_calls(&f, 2);
    f.ci.finish(passed(&["CI passed: CI u"]));
    f.settle();
    assert_eq!(f.task(&task.id).state, TaskState::Review);
}

#[test]
fn fails_ci_after_a_restart_with_no_workspace() {
    fails_without(true, false);
}

#[test]
fn fails_ci_after_a_restart_with_no_branch() {
    fails_without(false, true);
}

#[test]
fn fails_ci_after_a_restart_with_no_workspace_and_branch() {
    fails_without(true, true);
}

// An empty worktree would run git in the daemon's own folder and push that checkout.
fn fails_without(no_workspace: bool, no_branch: bool) {
    let mut f = fixture();
    let task = reported(&f, true);
    wait_for_calls(&f, 1);

    f.runner.stop();
    f.tasks
        .record_workspace(
            &task.id,
            WorkspaceChange {
                workspace: if no_workspace {
                    None
                } else {
                    task.workspace.clone()
                },
                branch: if no_branch { None } else { task.branch.clone() },
            },
        )
        .unwrap();
    f.bus.deliver_all();
    f.runner = f.make_runner(None);
    f.runner.start();
    f.settle();

    assert_eq!(f.ci.calls.lock().unwrap().len(), 1);
    assert_eq!(
        Value::Object(payload_of(&f, &task.id, "stage.check-failed")),
        json!({
            "stage": "implement",
            "reasons": ["The task has no workspace or branch for the CI check."],
            "final": true,
        })
    );
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    assert!(f.last_of(&task.id, "stage.reopened").is_none());
    assert_eq!(count_of(&f, &task.id, "stage.check-failed"), 1);

    f.runner.stop();
    f.runner = f.make_runner(None);
    f.runner.start();
    f.settle();
    assert_eq!(f.ci.calls.lock().unwrap().len(), 1);
    assert_eq!(count_of(&f, &task.id, "stage.check-failed"), 1);
}

#[test]
fn does_not_wait_for_ci_when_the_task_did_not_ask_for_it() {
    let f = fixture();
    let task = reported(&f, false);

    assert!(f.ci.calls.lock().unwrap().is_empty());
    assert_eq!(f.task(&task.id).state, TaskState::Review);
}
