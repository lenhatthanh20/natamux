//! `describe('a stage of a group marked to skip')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::{Value, json};

use crate::Fixture;

fn at_plan_review(f: &Fixture) -> Task {
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "plan" } }),
    );
    f.with_workspace(&task.id)
}

fn approve(f: &Fixture, id: &str) -> Task {
    f.runner
        .run
        .act(id, StageAction::Approve, &StageActionInput::default())
        .unwrap()
}

#[test]
fn is_skipped_and_the_other_stage_of_the_group_starts() {
    let f = Fixture::new();
    let task = at_plan_review(&f);
    f.set_skip(&task.id, "test-spec", true);

    approve(&f, &task.id);
    f.settle();

    assert_eq!(
        Value::Object(f.task(&task.id).lanes),
        json!({ "test-spec": "done", "implement": "running" })
    );
    // No agent started for the skipped stage.
    assert!(
        std::fs::read_to_string(f.data_dir.join(format!("agent-launch-{}-2.sh", task.id))).is_err()
    );
}

#[test]
fn skips_a_group_and_the_stage_after_it_in_one_move_and_answers_with_the_next_stage_started() {
    let f = Fixture::new();
    let task = at_plan_review(&f);
    for stage in ["test-spec", "implement", "testing"] {
        f.set_skip(&task.id, stage, true);
    }

    let moved = approve(&f, &task.id);

    assert_eq!(
        (moved.stage.as_str(), moved.state),
        ("agent-review", TaskState::Running)
    );
    let skipped: Vec<Value> = f
        .task_events(&task.id)
        .into_iter()
        .filter(|event| event.kind == "stage.skipped")
        .map(|event| event.payload["stage"].clone())
        .collect();
    assert_eq!(
        skipped,
        vec![json!("test-spec"), json!("implement"), json!("testing")]
    );
}

#[test]
fn skips_the_whole_group_when_each_of_its_stages_is_marked() {
    let f = Fixture::new();
    let task = at_plan_review(&f);
    f.set_skip(&task.id, "test-spec", true);
    f.set_skip(&task.id, "implement", true);

    let moved = approve(&f, &task.id);

    assert_eq!(moved.stage, "testing");
    f.settle();
    assert_eq!(f.task(&task.id).stage, "testing");
}
