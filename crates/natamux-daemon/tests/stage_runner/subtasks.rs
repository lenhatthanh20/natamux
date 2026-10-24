//! `describe('a parent with subtasks')` of `stage-runner.test.ts`.

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::json;

use crate::context::set_stages;
use crate::{Fixture, run_git};

/// The `beforeEach` of the group.
fn fixture() -> Fixture {
    let f = Fixture::new();
    set_stages(
        &f,
        vec![
            Stage::new("implement", StageKind::Agent, Gate::Manual),
            Stage::new("review", StageKind::Review, Gate::Manual),
        ],
    );
    f
}

fn start(f: &Fixture, id: &str) -> Task {
    f.runner
        .run
        .act(id, StageAction::Start, &StageActionInput::default())
        .unwrap()
}

/// A parent whose Implement waits to start in its workspace, and one draft subtask.
fn waiting_parent(f: &Fixture) -> (Task, Task) {
    let task = f.created(json!({ "title": "Export", "agent": "claude-code" }));
    let child = f
        .tasks
        .create_task(
            Some(&json!({ "title": "Part A", "parentId": task.id })),
            &[],
            true,
        )
        .unwrap();
    f.with_workspace(&task.id);
    let parent = start(f, &task.id);
    f.settle();
    (f.task(&parent.id), child)
}

fn finish(f: &Fixture, id: &str) {
    f.apply(
        id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "done" } }),
    );
}

/// The remote copy of the parent's branch got a merged subtask.
fn merged_on_remote(f: &Fixture, parent: &Task) {
    let origin = f.add_bare_origin();
    let workspace = std::path::PathBuf::from(parent.workspace.as_deref().unwrap());
    let branch = parent.branch.as_deref().unwrap();
    run_git(&workspace, &["push", "-u", "origin", branch]);
    let other = f.dir.0.join("other");
    let origin_text = origin.to_string_lossy().into_owned();
    let other_text = other.to_string_lossy().into_owned();
    run_git(
        &f.dir.0,
        &["clone", "--branch", branch, &origin_text, &other_text],
    );
    run_git(&other, &["commit", "--allow-empty", "-m", "merged subtask"]);
    run_git(&other, &["push", "origin", branch]);
}

#[test]
fn starts_a_new_stage_once_its_open_subtasks_are_done() {
    let f = fixture();
    let (parent, child) = waiting_parent(&f);
    assert_eq!(
        (parent.stage.as_str(), parent.state),
        ("implement", TaskState::Draft)
    );
    assert!(f.ptys().is_empty());

    finish(&f, &child.id);

    assert_eq!(f.task(&parent.id).state, TaskState::Running);
    assert_eq!(f.ptys().len(), 1);
}

#[test]
fn stops_waiting_when_its_subtask_is_cancelled_or_deleted() {
    let f = fixture();
    let (parent, child) = waiting_parent(&f);
    let other = f
        .tasks
        .create_task(
            Some(&json!({ "title": "Part B", "parentId": parent.id })),
            &[],
            true,
        )
        .unwrap();

    f.apply(
        &child.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );
    assert_eq!(f.task(&parent.id).state, TaskState::Draft);

    f.tasks.delete_task(&other.id).unwrap();
    f.bus.deliver_all();
    f.settle();
    assert_eq!(f.task(&parent.id).state, TaskState::Running);
}

#[test]
fn does_not_stop_a_stage_that_runs_when_a_subtask_is_added() {
    let f = fixture();
    let task = f.created(json!({ "title": "Export", "agent": "claude-code" }));
    f.with_workspace(&task.id);
    start(&f, &task.id);
    f.settle();
    assert_eq!(f.task(&task.id).state, TaskState::Running);
    f.tasks
        .create_task(
            Some(&json!({ "title": "Part A", "parentId": task.id })),
            &[],
            true,
        )
        .unwrap();
    f.bus.deliver_all();
    f.settle();

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(!f.pty(0).killed());
}

#[test]
fn pulls_the_merged_work_of_its_subtasks_before_the_stage_starts() {
    let f = fixture();
    let (parent, child) = waiting_parent(&f);
    merged_on_remote(&f, &parent);

    finish(&f, &child.id);

    assert_eq!(f.task(&parent.id).state, TaskState::Running);
    let workspace = std::path::PathBuf::from(parent.workspace.as_deref().unwrap());
    assert_eq!(
        run_git(&workspace, &["log", "-1", "--format=%s"]).trim(),
        "merged subtask"
    );
}

#[test]
fn fails_the_stage_when_the_pull_fails() {
    let f = fixture();
    let (parent, child) = waiting_parent(&f);
    merged_on_remote(&f, &parent);
    let workspace = std::path::PathBuf::from(parent.workspace.as_deref().unwrap());
    run_git(&workspace, &["commit", "--allow-empty", "-m", "local only"]);

    finish(&f, &child.id);

    let failed = f
        .events(&parent.id)
        .into_iter()
        .find(|(kind, _)| kind == "stage.check-failed")
        .unwrap()
        .1;
    assert!(
        failed["reasons"]
            .to_string()
            .contains("Could not pull the work of the subtasks")
    );
    assert!(f.ptys().is_empty());
}
