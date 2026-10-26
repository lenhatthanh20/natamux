//! `describe('a CI failure on a testing stage')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::ci::CiResult;
use serde_json::{Value, json};

use crate::{Fixture, SEND_BACK_LIMIT, expect_submitted, term_input, wait_for};

fn fixture() -> Fixture {
    let f = Fixture::new();
    f.set_stages(json!([
        { "id": "implement", "kind": "agent", "gate": "auto" },
        { "id": "testing", "kind": "testing", "gate": "auto", "ci": true },
        { "id": "release", "kind": "release", "gate": "manual" },
    ]));
    f.set_send_back_limit(Some(SEND_BACK_LIMIT));
    f
}

fn failure() -> CiResult {
    CiResult {
        passed: false,
        reasons: vec!["CI E2E failure: u".into()],
        logs: Some(vec!["CI E2E failure: u\nError: 2 tests failed".into()]),
        passes: vec![],
        final_: false,
    }
}

fn report(f: &Fixture, id: &str) {
    f.runner
        .run
        .act(id, StageAction::Report, &term_input("1"))
        .unwrap();
}

// Implement is done, the agent of Automated tests reports, and CI ends with `result`.
fn fail_on_ci(f: &Fixture, id: &str, result: CiResult) {
    report(f, id);
    f.settle();
    assert_eq!(f.task(id).stage, "testing");
    report(f, id);
    f.ci.finish(result);
    f.settle();
}

fn with_ci(f: &Fixture) -> Task {
    let task = f.implementing("implement");
    f.set_ci(&task.id, "testing", true);
    task
}

fn needs_input_at_testing(f: &Fixture, id: &str) -> bool {
    let task = f.task(id);
    (task.stage.as_str(), task.state) == ("testing", TaskState::NeedsInput)
}

#[test]
fn sends_the_task_back_to_implement_with_the_failed_runs_not_their_logs() {
    let f = fixture();
    let task = with_ci(&f);

    fail_on_ci(&f, &task.id, failure());

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("implement", TaskState::Running)
    );
    assert_eq!(
        Value::Object(f.last_of(&task.id, "stage.check-failed").unwrap().payload),
        json!({ "stage": "testing", "reasons": ["CI E2E failure: u"], "final": false })
    );
    let reopened = f.last_of(&task.id, "stage.reopened").unwrap().payload;
    assert_eq!(reopened["to"], "implement");
    assert_eq!(reopened["from"], "testing");
    assert_eq!(reopened["byAgent"], true);
    assert_eq!(reopened["ci"], true);
    let reason = reopened["reason"].as_str().unwrap();
    assert!(reason.contains("CI E2E failure: u"));
    assert!(reason.contains("gh run view"));
    assert!(!reason.contains("Error: 2 tests failed"));
    wait_for(|| f.pty(0).written().concat().contains("gh run view"));
    assert!(
        !f.pty(0)
            .written()
            .concat()
            .contains("The checks of this stage failed")
    );
}

#[test]
fn counts_the_send_backs_of_the_testing_agent_toward_the_most_times() {
    let f = fixture();
    let task = with_ci(&f);
    report(&f, &task.id);
    f.settle();
    let input = StageActionInput {
        reason: Some("Add a test for dates".into()),
        ..term_input("1")
    };
    f.runner
        .run
        .act(&task.id, StageAction::Reopen, &input)
        .unwrap();
    f.settle();
    assert_eq!(f.task(&task.id).stage, "implement");

    fail_on_ci(&f, &task.id, failure());
    assert_eq!(f.task(&task.id).stage, "implement");

    fail_on_ci(&f, &task.id, failure());
    assert!(needs_input_at_testing(&f, &task.id));
}

#[test]
fn waits_for_you_after_it_sent_the_task_back_the_most_times() {
    let f = fixture();
    let task = with_ci(&f);
    for _ in 0..SEND_BACK_LIMIT {
        fail_on_ci(&f, &task.id, failure());
        assert_eq!(f.task(&task.id).stage, "implement");
    }

    fail_on_ci(&f, &task.id, failure());

    assert!(needs_input_at_testing(&f, &task.id));
    assert_eq!(
        Value::Object(f.last_of(&task.id, "stage.check-failed").unwrap().payload),
        json!({ "stage": "testing", "reasons": ["CI E2E failure: u"], "final": true })
    );
}

#[test]
fn sends_each_ci_failure_back_when_no_limit_is_set() {
    let f = fixture();
    f.set_send_back_limit(None);
    let task = with_ci(&f);
    for _ in 0..SEND_BACK_LIMIT + 2 {
        fail_on_ci(&f, &task.id, failure());
        assert_eq!(f.task(&task.id).stage, "implement");
    }
    assert!(!needs_input_at_testing(&f, &task.id));
}

#[test]
fn waits_for_you_when_no_agent_can_fix_it() {
    let f = fixture();
    let task = with_ci(&f);

    fail_on_ci(
        &f,
        &task.id,
        CiResult {
            final_: true,
            ..failure()
        },
    );

    assert!(needs_input_at_testing(&f, &task.id));
    assert!(f.last_of(&task.id, "stage.reopened").is_none());
}

#[test]
fn gives_the_failure_to_its_own_agent_when_no_stage_before_it_changes_code() {
    let f = fixture();
    f.set_stages(json!([{ "id": "testing", "kind": "testing", "gate": "auto", "ci": true }]));
    let task = f.implementing("testing");
    f.set_ci(&task.id, "testing", true);

    report(&f, &task.id);
    f.ci.finish(failure());
    f.settle();

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("testing", TaskState::Running)
    );
    expect_submitted(&f.pty(0), "The checks of this stage failed");
    assert!(
        f.pty(0)
            .written()
            .concat()
            .contains("Error: 2 tests failed")
    );
}
