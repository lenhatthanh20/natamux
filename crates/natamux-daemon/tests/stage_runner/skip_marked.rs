//! `describe('a stage marked to skip')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::{Fixture, term_input};

fn fixture() -> Fixture {
    let f = Fixture::new();
    f.set_stages(json!([
        { "id": "implement", "kind": "agent", "gate": "auto" },
        { "id": "qa", "kind": "manual", "gate": "manual" },
        { "id": "docs", "kind": "agent", "gate": "auto" },
        { "id": "release", "kind": "release", "gate": "manual" },
    ]));
    f
}

fn skips(f: &Fixture, id: &str) -> Vec<natamux_proto::events::task_event::TaskEvent> {
    f.task_events(id)
        .into_iter()
        .filter(|event| event.kind == "stage.skipped")
        .collect()
}

fn report_implement(f: &Fixture, id: &str) -> Task {
    f.runner
        .run
        .act(id, StageAction::Report, &term_input("1"))
        .unwrap();
    f.settle();
    f.task(id)
}

fn is(task: &Task, stage: &str, state: TaskState) -> bool {
    task.stage == stage && task.state == state
}

#[test]
fn is_skipped_when_the_task_reaches_it_and_the_next_stage_runs() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_skip(&task.id, "qa", true);

    assert!(is(
        &report_implement(&f, &task.id),
        "docs",
        TaskState::Running
    ));
    let skipped = skips(&f, &task.id);
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].source, "system");
    assert_eq!(
        Value::Object(skipped[0].payload.clone()),
        json!({ "stage": "qa", "planned": true })
    );
}

#[test]
fn is_skipped_again_after_a_send_back_but_runs_when_sent_back_to_it() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_skip(&task.id, "qa", true);
    report_implement(&f, &task.id);
    let run = &f.runner.run;

    let back = StageActionInput {
        to: Some("implement".into()),
        reason: Some("Fix the dates".into()),
        ..Default::default()
    };
    run.act(&task.id, StageAction::Reopen, &back).unwrap();
    f.settle();
    assert_eq!(f.task(&task.id).stage, "implement");
    assert!(is(
        &report_implement(&f, &task.id),
        "docs",
        TaskState::Running
    ));
    assert_eq!(skips(&f, &task.id).len(), 2);

    let to_qa = StageActionInput {
        to: Some("qa".into()),
        ..Default::default()
    };
    run.act(&task.id, StageAction::Reopen, &to_qa).unwrap();
    f.settle();
    assert!(is(&f.task(&task.id), "qa", TaskState::Review));
    assert_eq!(skips(&f, &task.id).len(), 2);
}

#[test]
fn is_skipped_after_pass_of_the_stage_before() {
    skipped_after(StageAction::Pass);
}

#[test]
fn is_skipped_after_skip_of_the_stage_before() {
    skipped_after(StageAction::Skip);
}

fn skipped_after(action: StageAction) {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_skip(&task.id, "qa", true);

    f.runner
        .run
        .act(&task.id, action, &StageActionInput::default())
        .unwrap();
    f.settle();

    assert!(is(&f.task(&task.id), "docs", TaskState::Running));
}

#[test]
fn is_skipped_after_a_review_approval_in_order_and_a_marked_release_ends_the_task() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_skip(&task.id, "docs", true);
    f.set_skip(&task.id, "release", true);
    assert!(is(&report_implement(&f, &task.id), "qa", TaskState::Review));

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "review.approved", "payload": { "stage": "qa" } }),
    );

    assert_eq!(f.task(&task.id).state, TaskState::Done);
    let moves: Vec<(String, Value)> = f
        .task_events(&task.id)
        .into_iter()
        .filter(|event| ["review.approved", "stage.skipped"].contains(&event.kind.as_str()))
        .map(|event| (event.kind, event.payload["stage"].clone()))
        .collect();
    assert_eq!(
        moves,
        vec![
            ("review.approved".to_string(), json!("qa")),
            ("stage.skipped".to_string(), json!("docs")),
            ("stage.skipped".to_string(), json!("release")),
        ]
    );
    assert!(f.tasks.set_skip(&task.id, "release", false).is_err());
}

#[test]
fn runs_when_the_mark_is_taken_off() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_skip(&task.id, "qa", true);
    f.set_skip(&task.id, "qa", false);

    assert!(is(&report_implement(&f, &task.id), "qa", TaskState::Review));
    assert!(skips(&f, &task.id).is_empty());
}
