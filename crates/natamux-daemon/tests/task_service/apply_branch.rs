//! The group 'applyEvent at a branch'.

use natamux_core::events::derive::derive_task_state;
use natamux_daemon::tasks::helpers::position_of;

use super::*;

fn branch_stages(docs: bool) -> Vec<Step> {
    let mut small = vec![json!({ "id": "implement", "kind": "agent", "gate": "auto" })];
    if docs {
        small.push(json!({ "id": "docs", "kind": "agent", "gate": "auto" }));
    }
    steps(json!([
        { "id": "plan", "kind": "agent", "gate": "manual" },
        {
            "id": "after-plan",
            "paths": [
                { "id": "small", "when": { "type": "command", "run": "true" }, "stages": small },
                { "id": "pr-review", "stages": [{ "id": "review", "kind": "review", "gate": "manual" }] }
            ]
        },
        { "id": "release", "kind": "release", "gate": "manual" }
    ]))
}

fn approved_plan(staged: &TaskService) -> Task {
    let task = create(staged, "Export");
    override_to(staged, &task.id, "review", Some("plan"));
    apply(
        staged,
        &task.id,
        json!({
            "source": "human", "type": "stage.approved",
            "payload": { "stage": "plan", "path": "small", "because": "true exited 0" }
        }),
    )
    .task
}

#[test]
fn starts_a_task_with_no_path_taken() {
    let fx = Fixture::new();
    let staged = fx.make(Some(settings(branch_stages(false))));

    assert_eq!(create(&staged, "Export").paths, obj(json!({})));
}

#[test]
fn stores_the_path_the_task_took_in_its_file_and_after_a_restart() {
    let fx = Fixture::new();
    let staged = fx.make(Some(settings(branch_stages(false))));

    let task = approved_plan(&staged);

    assert_eq!(task.stage, "implement");
    assert_eq!(task.paths, obj(json!({ "after-plan": "small" })));
    assert_eq!(
        file_task(&fx, &task.id).unwrap().paths,
        obj(json!({ "after-plan": "small" }))
    );
    let restarted = fx.started_on(fx.memory_index(), Some(settings(branch_stages(false))));
    assert_eq!(
        restarted.get_task(&task.id).unwrap().paths,
        obj(json!({ "after-plan": "small" }))
    );
}

#[test]
fn keeps_the_path_on_later_moves_and_the_log_gives_the_same_position() {
    let fx = Fixture::new();
    let staged = fx.make(Some(settings(branch_stages(false))));
    let task = approved_plan(&staged);

    let moved = apply(
        &staged,
        &task.id,
        json!({ "source": "human", "type": "stage.passed", "payload": { "stage": "implement" } }),
    )
    .task;

    assert_eq!(moved.stage, "release");
    assert_eq!(moved.paths, obj(json!({ "after-plan": "small" })));
    let events: Vec<TaskEvent> = staged
        .list_events(&task.id)
        .unwrap()
        .into_iter()
        .map(|entry| entry.event)
        .collect();
    let (position, _) = derive_task_state(&events, &staged.steps_of(&moved).unwrap());
    assert_eq!(position, position_of(&moved));
}

#[test]
fn forgets_the_path_when_the_task_goes_back_before_the_branch() {
    let fx = Fixture::new();
    let staged = fx.make(Some(settings(branch_stages(false))));
    let task = approved_plan(&staged);

    let back = apply(
        &staged,
        &task.id,
        json!({ "source": "human", "type": "stage.reopened", "payload": { "to": "plan" } }),
    )
    .task;

    assert_eq!(back.stage, "plan");
    assert_eq!(back.paths, obj(json!({})));
}

#[test]
fn marks_a_later_stage_of_the_path_the_task_is_on_but_not_the_current_one() {
    let fx = Fixture::new();
    let staged = fx.make(Some(settings(branch_stages(true))));
    let task = approved_plan(&staged);

    assert!(err_text(staged.set_skip(&task.id, "implement", true)).contains("Skip"));
    assert_eq!(
        staged.set_skip(&task.id, "docs", true).unwrap().skip_stages,
        Some(vec!["docs".to_string()])
    );
}
