//! The group 'AgentHookService, sign of life', on a clock the test moves.

use std::sync::atomic::AtomicI64;

use super::*;

struct Life {
    fx: Fx,
    time: Arc<AtomicI64>,
    service: HookService,
}

fn setup() -> Life {
    let (time, clock) = manual_clock();
    let fx = Fx::build(None, Some(clock));
    let service = fx.service();
    Life { fx, time, service }
}

impl Life {
    fn later(&self, seconds: i64) {
        self.time.fetch_add(seconds * 1000, Ordering::SeqCst);
    }

    fn updated_at(&self, id: &str) -> String {
        self.fx.task(id).updated_at
    }

    fn running_task(&self) -> Task {
        let task = self.fx.make("Fix it");
        step(&self.service, &task.id, "UserPromptSubmit");
        task
    }
}

#[test]
fn records_a_sign_of_life_when_a_hook_of_a_running_task_changes_nothing() {
    let l = setup();
    let task = l.running_task();
    l.later(45);
    let logged = l.fx.tasks.list_events(&task.id).unwrap();

    step(&l.service, &task.id, "PostToolUse");

    assert_eq!(l.updated_at(&task.id), "2026-10-08T09:00:45.000Z");
    // Only the time moves: the log, which the dashboard reads again, stays as it was.
    assert_eq!(l.fx.tasks.list_events(&task.id).unwrap(), logged);
}

#[test]
fn moves_the_time_at_most_once_per_interval_when_hooks_come_quickly_one_after_the_other() {
    let l = setup();
    let task = l.running_task();
    l.later(45);
    step(&l.service, &task.id, "PostToolUse");

    for _ in 0..5 {
        l.later(2);
        step(&l.service, &task.id, "PostToolUse");
    }

    assert_eq!(l.updated_at(&task.id), "2026-10-08T09:00:45.000Z");
}

#[test]
fn keeps_the_time_the_wait_for_background_work_began() {
    let l = setup();
    let task = l.running_task();
    let paused = hook(
        "Stop",
        json!({ "session_crons": [{ "id": "c1", "prompt": "check CI" }] }),
    );
    l.later(10);
    claude(&l.service, &task.id, &paused, None);
    l.later(300);
    claude(&l.service, &task.id, &paused, None);

    assert_eq!(
        l.fx.task(&task.id).background_since.as_deref(),
        Some("2026-10-08T09:00:10.000Z")
    );
}

#[test]
fn ends_the_wait_at_the_first_sign_of_life_even_soon_after_it_began() {
    let l = setup();
    let task = l.running_task();
    let paused = hook(
        "Stop",
        json!({ "background_tasks": [{ "type": "shell", "command": "pnpm test" }] }),
    );
    claude(&l.service, &task.id, &paused, None);
    l.later(2);

    step(&l.service, &task.id, "PostToolUse");

    let after = l.fx.task(&task.id);
    assert!(after.background_since.is_none());
    assert_eq!(after.updated_at, "2026-10-08T09:00:02.000Z");
}

#[test]
fn records_nothing_for_a_hook_the_adapter_does_not_use_or_for_a_task_that_is_not_running() {
    let l = setup();
    let task = l.fx.make("Fix it");
    l.later(3600);

    step(&l.service, &task.id, "Stop");
    step(&l.service, &task.id, "SessionEnd");

    assert_eq!(l.updated_at(&task.id), task.updated_at);
}
