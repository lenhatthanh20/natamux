//! `describe('a send back')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_daemon::stages::errors::StageError;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::support::SIZE;
use crate::{Fixture, wait_for};

// The default pipeline with a person's review in place of the agent's.
fn fixture() -> Fixture {
    let f = Fixture::new();
    let stages = default_pipeline()
        .into_iter()
        .map(|step| match &step {
            Step::Stage(stage) if stage.id == "agent-review" => {
                Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual))
            }
            _ => step,
        })
        .collect();
    f.set_settings(natamux_proto::projects::repo::ProjectSettings {
        stages,
        ..Default::default()
    });
    f
}

fn in_review(f: &Fixture) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "review" } }),
    );
    f.with_workspace(&task.id)
}

fn types_since(f: &Fixture, id: &str, kind: &str) -> Vec<String> {
    let types: Vec<String> = f
        .task_events(id)
        .into_iter()
        .map(|event| event.kind)
        .collect();
    let at = types.iter().rposition(|each| each == kind).unwrap_or(0);
    types[at..].to_vec()
}

fn reopen_to(to: &str) -> StageActionInput {
    StageActionInput {
        to: Some(to.into()),
        ..Default::default()
    }
}

#[test]
fn starts_the_code_stage_again_with_the_changes_asked_for() {
    let f = fixture();
    let task = in_review(&f);

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "review.changes-requested", "payload": { "reason": "Fix the dates" } }),
    );

    let now = f.task(&task.id);
    assert_eq!(now.stage, "build");
    assert_eq!(
        Value::Object(now.lanes),
        json!({ "test-spec": "done", "implement": "running" })
    );
    assert_eq!(
        types_since(&f, &task.id, "review.changes-requested"),
        [
            "review.changes-requested",
            "stage.started",
            "agent.launched"
        ]
    );
    assert!(
        f.launch_script(&task.id, "1")
            .contains("What to change: Fix the dates")
    );
}

#[test]
fn takes_the_stage_up_again_with_no_new_prompt_when_you_type_to_the_agent() {
    let f = fixture();
    let task = in_review(&f);
    f.terminals.run(&task.id, "claude", "1", SIZE).unwrap();
    let pty = f.ptys().pop().unwrap();
    pty.set_program(Some("node"));

    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.resumed", "payload": { "stage": "review" } }),
    );

    assert_eq!(
        Value::Object(f.task(&task.id).lanes),
        json!({ "test-spec": "done", "implement": "running" })
    );
    assert_eq!(
        types_since(&f, &task.id, "agent.resumed"),
        ["agent.resumed", "stage.started"]
    );
    wait_for(|| pty.written() == ["claude\r"]);
    assert_eq!(pty.written(), ["claude\r"]);
}

#[test]
fn starts_the_agent_again_when_you_resume_the_review_and_no_agent_runs() {
    let f = fixture();
    let task = in_review(&f);

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "agent.resumed" }),
    );

    assert_eq!(
        Value::Object(f.task(&task.id).lanes),
        json!({ "test-spec": "done", "implement": "running" })
    );
    assert_eq!(
        types_since(&f, &task.id, "agent.resumed"),
        ["agent.resumed", "stage.started", "agent.launched"]
    );
}

#[test]
fn refuses_a_stage_the_task_does_not_have_or_has_not_reached() {
    let f = fixture();
    let task = in_review(&f);
    for to in ["nope", "release"] {
        let error = f
            .runner
            .run
            .act(&task.id, StageAction::Reopen, &reopen_to(to))
            .unwrap_err();
        assert!(matches!(error, StageError::Input(_)), "{to}: {error}");
    }
}

#[test]
fn answers_with_the_task_after_its_stage_failed_to_start() {
    let f = fixture();
    let task =
        f.created(json!({ "title": "Export", "context": { "source": "github-issue", "ref": "" } }));
    assert_eq!(task.state, TaskState::NeedsInput);

    let after = f
        .runner
        .run
        .act(&task.id, StageAction::Reopen, &reopen_to("context"))
        .unwrap();

    assert_eq!(after.state, TaskState::NeedsInput);
    assert_eq!(
        types_since(&f, &task.id, "stage.reopened"),
        ["stage.reopened", "stage.started", "stage.check-failed"]
    );
}
