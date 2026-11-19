//! The pinned terminals of a lookup come from a narrow read of the task's events. They must be
//! the ones the whole event list gives.

use natamux_core::stages::terminal_lane::pinned_terminals_of;
use natamux_proto::tasks::task::TaskState;
use serde_json::json;

use crate::Fixture;

#[test]
fn the_pinned_terminals_of_a_lookup_are_those_of_the_whole_event_list() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "build" } }),
    );
    let task = f.with_workspace(&task.id);
    let run = &f.runner.run;

    let events = run.options.tasks.list_events(&task.id).unwrap();
    let wanted = pinned_terminals_of(events.iter().map(|stored| (stored.accepted, &stored.event)));

    // The group pinned a terminal for each of its stages.
    assert!(wanted.len() >= 2);
    assert_eq!(run.pinned(&task.id), wanted);
    assert_eq!(
        run.lane_for(&task, Some("2")).unwrap(),
        Some(("test-spec".to_string(), TaskState::Running))
    );
    // A task that is gone pins nothing, as a failed read of its events did.
    assert!(run.pinned("T-9999").is_empty());
}

#[test]
fn the_steps_and_the_end_of_a_task_are_read_as_before() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "build" } }),
    );
    let task = f.task(&task.id);
    let run = &f.runner.run;

    let steps = run.steps(&task).unwrap();
    assert_eq!(
        run.step_of(&task).unwrap(),
        natamux_daemon::stages::run::step_in(&task, &steps).unwrap()
    );
    let tasks = &run.options.tasks;
    assert_eq!(tasks.is_finished(&task.id).unwrap(), Some(false));
    assert_eq!(tasks.is_finished("T-9999").unwrap(), None);
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );
    assert_eq!(tasks.is_finished(&task.id).unwrap(), Some(true));
}
