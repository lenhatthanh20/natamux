//! `describe('touch')`. Node's fake timers are the manual `Scheduler` of the fixture: `advance`
//! runs the timers of `workspace.dirty`. The messages are what the workspace bus gave.

use std::fs;

use natamux_daemon::workspaces::dirty::{DIRTY_BURST, DIRTY_DEBOUNCE_MS, DIRTY_MIN_INTERVAL_MS};

use super::*;

fn dirty(id: &str) -> DaemonMessage {
    DaemonMessage::WorkspaceDirty {
        task_id: id.to_string(),
    }
}

fn burst() -> usize {
    DIRTY_BURST as usize
}

#[test]
fn tells_the_dashboards_once_per_burst_of_changes_in_a_task() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    for _ in 0..10 {
        fx.ws.touch(&task.id);
        fx.scheduler.advance(50);
    }
    assert!(fx.sent.lock().unwrap().is_empty());
    fx.scheduler.advance(DIRTY_DEBOUNCE_MS - 500);
    assert_eq!(*fx.sent.lock().unwrap(), [dirty(&task.id)]);
}

#[test]
fn tells_a_few_changes_in_a_row_soon_as_when_a_shell_starts_and_runs_a_command() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    for change in 1..=burst() {
        fx.ws.touch(&task.id);
        fx.scheduler.advance(DIRTY_DEBOUNCE_MS);
        assert_eq!(fx.dirty_count(&task.id), change);
        fx.scheduler.advance(1000);
    }
}

#[test]
fn tells_once_per_interval_while_changes_go_on_and_once_after_the_last() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    fx.ws.touch(&task.id);
    fx.scheduler.advance(DIRTY_DEBOUNCE_MS);
    assert_eq!(fx.dirty_count(&task.id), 1);

    // A busy agent: a change every second for 25 seconds. The first ones are told soon, then one
    // per interval.
    for _ in 0..25 {
        fx.ws.touch(&task.id);
        fx.scheduler.advance(1000);
    }
    assert_eq!(fx.dirty_count(&task.id), burst() + 2);

    // The changes after the last message are told once, at the end of the interval.
    fx.scheduler.advance(DIRTY_MIN_INTERVAL_MS);
    assert_eq!(fx.dirty_count(&task.id), burst() + 3);
    fx.scheduler
        .advance(DIRTY_MIN_INTERVAL_MS * (u64::from(DIRTY_BURST) + 1));
    assert_eq!(fx.dirty_count(&task.id), burst() + 3);

    // After a quiet time, the first change is told soon again.
    fx.ws.touch(&task.id);
    fx.scheduler.advance(DIRTY_DEBOUNCE_MS);
    assert_eq!(fx.dirty_count(&task.id), burst() + 4);
}

#[test]
fn tells_about_each_task_apart() {
    let fx = Fx::new();
    let first = fx.task("First");
    let second = fx.task("Second");

    fx.ws.touch(&first.id);
    fx.ws.touch(&second.id);
    fx.ws.touch(&first.id);
    fx.scheduler.advance(DIRTY_DEBOUNCE_MS);

    assert_eq!(
        *fx.sent.lock().unwrap(),
        [dirty(&first.id), dirty(&second.id)]
    );
}

#[test]
fn says_nothing_about_a_task_deleted_in_the_meantime() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    fx.ws.touch(&task.id);
    fx.tasks.delete_task(&task.id).unwrap();
    fx.scheduler.advance(DIRTY_DEBOUNCE_MS);

    assert_eq!(fx.dirty_count(&task.id), 0);
}

#[test]
fn says_nothing_about_a_task_deleted_while_it_waited_for_the_next_interval() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    for _ in 0..DIRTY_BURST {
        fx.ws.touch(&task.id);
        fx.scheduler.advance(DIRTY_DEBOUNCE_MS);
    }

    fx.ws.touch(&task.id);
    fx.tasks.delete_task(&task.id).unwrap();
    fx.scheduler.advance(DIRTY_MIN_INTERVAL_MS * 2);

    assert_eq!(fx.dirty_count(&task.id), burst());
}

// Node spies on `touch`; here the effect is checked: a commit marks the workspace, so a
// `workspace.dirty` follows once the timer runs.
#[test]
fn follows_each_change_made_here_such_as_a_commit() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
    fs::write(
        PathBuf::from(created.workspace.unwrap()).join("notes.txt"),
        "a\n",
    )
    .unwrap();
    fx.flush_dirty();
    let before = fx.dirty_count(&task.id);

    fx.ws.commit(&task.id, "Add notes").unwrap();
    fx.flush_dirty();

    assert_eq!(fx.dirty_count(&task.id), before + 1);
}
