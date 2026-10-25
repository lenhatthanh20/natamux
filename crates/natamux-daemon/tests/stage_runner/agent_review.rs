//! `describe('an agent review')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::{Fixture, term_input};

fn fixture() -> Fixture {
    let f = Fixture::new();
    f.set_stages(json!([
        { "id": "implement", "kind": "agent", "gate": "auto" },
        {
            "id": "agent-review", "kind": "agent", "gate": "auto", "output": "review.md",
            "sections": ["Findings"], "ownShell": true,
        },
        { "id": "release", "kind": "release", "gate": "manual" },
    ]));
    f
}

fn at_review(f: &Fixture) -> Task {
    let task = f.implementing("implement");
    f.runner
        .run
        .act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();
    f.task(&task.id)
}

fn reopen(reason: &str, terminal: Option<&str>) -> StageActionInput {
    StageActionInput {
        terminal: terminal.map(String::from),
        reason: Some(reason.into()),
        ..Default::default()
    }
}

fn reports_of(f: &Fixture, id: &str, stage: &str) -> Vec<TaskEvent> {
    f.task_events(id)
        .into_iter()
        .filter(|event| event.kind == "stage.reported")
        .filter(|event| event.payload.get("stage").and_then(|s| s.as_str()) == Some(stage))
        .collect()
}

fn is(task: &Task, stage: &str, state: TaskState) -> bool {
    task.stage == stage && task.state == state
}

#[test]
fn starts_its_own_agent_in_shell_5_when_implement_is_done() {
    let f = fixture();
    let task = at_review(&f);

    assert!(is(&task, "agent-review", TaskState::Running));
    assert!(f.launch_script(&task.id, "5").contains("review.md"));
    assert!(
        f.launch_script(&task.id, "5")
            .contains("natamux stage back")
    );
}

#[test]
fn sends_the_task_back_to_implement_with_its_findings_twice_then_waits_for_you() {
    let f = fixture();
    let task = at_review(&f);
    let run = &f.runner.run;

    run.act(
        &task.id,
        StageAction::Reopen,
        &reopen("Fix the dates", Some("5")),
    )
    .unwrap();
    f.settle();
    assert!(is(&f.task(&task.id), "implement", TaskState::Running));
    assert!(f.pty(0).written().concat().contains("Fix the dates"));
    // The turn of the reviewer ends after it sent the task back, as after natamux done.
    assert!(f.runner.run.take_report(&task.id, "5"));

    run.act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();
    assert_eq!(f.task(&task.id).stage, "agent-review");
    run.act(
        &task.id,
        StageAction::Reopen,
        &reopen("Still wrong", Some("5")),
    )
    .unwrap();
    run.act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();

    let error = run
        .act(&task.id, StageAction::Reopen, &reopen("Again", Some("5")))
        .unwrap_err();
    assert!(error.to_string().contains("waits for the user"), "{error}");
    assert!(is(&f.task(&task.id), "agent-review", TaskState::NeedsInput));
}

#[test]
fn lets_a_person_send_it_back_any_number_of_times() {
    let f = fixture();
    let task = at_review(&f);
    for reason in ["one", "two", "three"] {
        f.runner
            .run
            .act(&task.id, StageAction::Reopen, &reopen(reason, None))
            .unwrap();
        f.runner
            .run
            .act(&task.id, StageAction::Report, &term_input("1"))
            .unwrap();
        f.settle();
    }
    assert_eq!(f.task(&task.id).stage, "agent-review");
}

#[test]
fn refuses_natamux_done_from_the_reviewer_after_it_sent_the_task_back() {
    let f = fixture();
    let task = at_review(&f);
    let run = &f.runner.run;
    run.act(
        &task.id,
        StageAction::Reopen,
        &reopen("Fix the dates", Some("5")),
    )
    .unwrap();
    f.settle();

    let error = run
        .act(&task.id, StageAction::Report, &term_input("5"))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("agent-review stage of Terminal 5 does not run now"),
        "{error}"
    );
    assert!(is(&f.task(&task.id), "implement", TaskState::Running));
    assert_eq!(reports_of(&f, &task.id, "implement").len(), 1);

    // The Implement agent still reports its own stage.
    run.act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();
    assert!(is(&f.task(&task.id), "agent-review", TaskState::Running));
}

#[test]
fn refuses_natamux_done_from_shell_1_while_the_reviewer_runs_in_its_own_shell() {
    let f = fixture();
    let task = at_review(&f);

    let error = f
        .runner
        .run
        .act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("No stage of Terminal 1 runs now"),
        "{error}"
    );
    assert!(is(&f.task(&task.id), "agent-review", TaskState::Running));
    assert!(reports_of(&f, &task.id, "agent-review").is_empty());
}

#[test]
fn takes_a_report_of_the_stage_named_from_any_shell() {
    let f = fixture();
    let task = at_review(&f);
    let run = &f.runner.run;
    run.act(
        &task.id,
        StageAction::Reopen,
        &reopen("Fix the dates", Some("5")),
    )
    .unwrap();
    f.settle();

    let input = StageActionInput {
        stage: Some("implement".into()),
        ..term_input("5")
    };
    run.act(&task.id, StageAction::Report, &input).unwrap();
    f.settle();

    assert!(is(&f.task(&task.id), "agent-review", TaskState::Running));
}

#[test]
fn refuses_the_other_actions_from_the_reviewer_after_its_send_back_unless_they_name_the_stage() {
    let f = fixture();
    let task = at_review(&f);
    let run = &f.runner.run;
    run.act(
        &task.id,
        StageAction::Reopen,
        &reopen("Fix the dates", Some("5")),
    )
    .unwrap();
    f.settle();

    for action in [
        StageAction::Approve,
        StageAction::Skip,
        StageAction::Pass,
        StageAction::Fail,
    ] {
        let error = run.act(&task.id, action, &term_input("5")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("agent-review stage of Terminal 5 does not run now"),
            "{error}"
        );
    }
    assert!(is(&f.task(&task.id), "implement", TaskState::Running));

    let input = StageActionInput {
        stage: Some("implement".into()),
        reason: Some("Stuck".into()),
        ..term_input("5")
    };
    let failed = run.act(&task.id, StageAction::Fail, &input).unwrap();
    assert!(is(&failed, "implement", TaskState::Failed));
}

#[test]
fn takes_a_report_from_a_shell_that_no_stage_uses_as_before() {
    let f = fixture();
    let task = at_review(&f);

    f.runner
        .run
        .act(&task.id, StageAction::Report, &term_input("3"))
        .unwrap();
    f.settle();

    // Reported and checked: review.md is missing, so the stage runs on.
    assert_eq!(reports_of(&f, &task.id, "agent-review").len(), 1);
    assert!(is(&f.task(&task.id), "agent-review", TaskState::Running));
}
