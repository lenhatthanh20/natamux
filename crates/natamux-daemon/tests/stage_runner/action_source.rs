//! `describe('who a stage action came from')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::TaskState;
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::Fixture;

fn source_of(f: &Fixture, id: &str, kind: &str) -> String {
    f.last_of(id, kind).unwrap().source
}

fn by_agent(by_agent: bool) -> StageActionInput {
    StageActionInput {
        by_agent,
        ..Default::default()
    }
}

// Rust has no undefined: the third row of the Node table is the same as the second.
#[test]
fn records_reopen_and_fail_with_by_agent_true_as_agent() {
    reopen_and_fail(true, "agent");
}

#[test]
fn records_reopen_and_fail_with_by_agent_false_as_human() {
    reopen_and_fail(false, "human");
}

fn reopen_and_fail(flag: bool, source: &str) {
    let f = Fixture::new();
    let task = f.implementing("implement");
    let run = &f.runner.run;
    let input = StageActionInput {
        to: Some("implement".into()),
        ..by_agent(flag)
    };
    run.act(&task.id, StageAction::Reopen, &input).unwrap();
    assert_eq!(source_of(&f, &task.id, "stage.reopened"), source);
    f.settle();
    run.act(&task.id, StageAction::Fail, &by_agent(flag))
        .unwrap();
    assert_eq!(source_of(&f, &task.id, "stage.failed"), source);
}

#[test]
fn records_a_report_with_by_agent_and_no_terminal_as_the_agent() {
    let f = Fixture::new();
    // The default pipeline runs test-spec beside implement, so a report with no name cannot pick one.
    f.set_stages(json!([{ "id": "implement", "kind": "agent", "gate": "manual" }]));
    let task = f.implementing("implement");
    f.runner
        .run
        .act(&task.id, StageAction::Report, &by_agent(true))
        .unwrap();
    f.settle();
    assert_eq!(source_of(&f, &task.id, "stage.reported"), "agent");
}

#[test]
fn records_a_report_with_no_terminal_and_no_by_agent_as_a_person() {
    let f = Fixture::new();
    // The default pipeline runs test-spec beside implement, so a report with no name cannot pick one.
    f.set_stages(json!([{ "id": "implement", "kind": "agent", "gate": "manual" }]));
    let task = f.implementing("implement");
    f.runner
        .run
        .act(&task.id, StageAction::Report, &StageActionInput::default())
        .unwrap();
    f.settle();
    assert_eq!(source_of(&f, &task.id, "stage.reported"), "human");
}

// A start records the stage starting as the system; it must never look like a person's step.
#[test]
fn records_nothing_as_a_person_for_a_start_by_an_agent() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "draft", "stage": "spec" } }),
    );
    f.with_workspace(&task.id);
    let before = f.task_events(&task.id).len();
    f.runner
        .run
        .act(&task.id, StageAction::Start, &by_agent(true))
        .unwrap();
    f.settle();
    let after: Vec<_> = f.task_events(&task.id).into_iter().skip(before).collect();
    assert!(after.iter().all(|event| event.source != "human"));
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("spec", TaskState::Running)
    );
}

// Security T-0346 (F-06): `natamux done` with the agent token passed a command stage that only
// its script, with its ticket, may pass.
#[test]
fn refuses_a_report_by_the_agent_token_on_a_command_stage() {
    const REFUSED: &str = "passes when its command passes";
    let f = Fixture::new();
    f.set_stages(json!([{ "id": "test", "kind": "testing", "gate": "auto", "command": "true" }]));
    let task = f.implementing("test");
    f.settle();
    let before = f.task(&task.id);

    let error = f
        .runner
        .run
        .act(&task.id, StageAction::Report, &by_agent(true))
        .unwrap_err();

    assert!(error.to_string().contains(REFUSED), "{error}");
    let now = f.task(&task.id);
    assert_eq!((now.stage.as_str(), now.state), ("test", before.state));
    assert!(f.last_of(&task.id, "stage.reported").is_none());
    // A person's report is not this refusal.
    if let Err(error) = f
        .runner
        .run
        .act(&task.id, StageAction::Report, &by_agent(false))
    {
        assert!(!error.to_string().contains(REFUSED), "{error}");
    }
}
