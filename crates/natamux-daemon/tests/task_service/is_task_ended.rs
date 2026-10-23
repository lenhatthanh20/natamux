//! The group 'isTaskEnded'. Node passed a fake with a `getTask`; `is_task_ended` takes the real
//! service, so the tests bring a real task to each state.

use natamux_daemon::tasks::service::is_task_ended;

use super::*;

#[test]
fn ends_the_shells_of_a_done_or_cancelled_task() {
    let fx = Fixture::new();
    let done = create(&fx.service, "Done");
    override_to(&fx.service, &done.id, "done", None);
    let cancelled = create(&fx.service, "Cancelled");
    human(&fx.service, &cancelled.id, "task.cancelled");

    assert!(is_task_ended(&fx.service, &done.id));
    assert!(is_task_ended(&fx.service, &cancelled.id));
}

#[test]
fn keeps_the_shells_of_a_task_that_is_still_active() {
    let fx = Fixture::new();
    let running = create(&fx.service, "Running");
    override_to(&fx.service, &running.id, "running", None);

    assert!(!is_task_ended(&fx.service, &running.id));
    assert!(!is_task_ended(
        &fx.service,
        &create(&fx.service, "Export").id
    ));
}

#[test]
fn ends_the_shells_of_a_task_that_is_gone() {
    let fx = Fixture::new();
    assert!(is_task_ended(&fx.service, "T-9999"));
}
