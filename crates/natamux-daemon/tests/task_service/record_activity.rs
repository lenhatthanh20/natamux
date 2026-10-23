//! The group 'recordActivity'.

use std::sync::atomic::{AtomicI64, Ordering};

use natamux_core::js::iso_from_millis;
use natamux_daemon::tasks::errors::TaskError;
use natamux_daemon::tasks::records::ActivityOptions;
use natamux_proto::tasks::task::TaskState;
use natamux_store::event_log::read_event_log;

use super::*;

struct Setup {
    fx: Fixture,
    time: Arc<AtomicI64>,
    service: TaskService,
}

impl Setup {
    fn new() -> Setup {
        let fx = Fixture::new();
        let (time, clock) = manual_clock(base_millis());
        let service = fx.with(Build {
            now: Some(clock),
            ..Build::default()
        });
        Setup { fx, time, service }
    }

    fn later(&self, seconds: i64) {
        self.time.fetch_add(seconds * 1000, Ordering::SeqCst);
    }

    fn started_task(&self) -> Task {
        let task = create(&self.service, "Fix it");
        human(&self.service, &task.id, "task.readied");
        apply(
            &self.service,
            &task.id,
            json!({ "source": "agent", "type": "agent.started" }),
        );
        task
    }

    fn activity(&self, id: &str) -> bool {
        self.service
            .record_activity(id, ActivityOptions::default())
            .unwrap()
    }

    fn updated_at(&self, id: &str) -> String {
        self.service.get_task(id).unwrap().updated_at
    }
}

#[test]
fn moves_the_time_of_the_last_update_of_a_running_task_with_no_line_in_the_log() {
    let s = Setup::new();
    let task = s.started_task();
    let logged = s.service.list_events(&task.id).unwrap();
    s.later(31);

    assert!(s.activity(&task.id));

    let updated = s.service.get_task(&task.id).unwrap();
    assert_eq!(updated.updated_at, "2026-10-08T09:00:31.000Z");
    assert_eq!(s.service.list_events(&task.id).unwrap(), logged);
    assert_eq!(
        read_event_log(s.fx.data(), &task.id)
            .unwrap()
            .unwrap()
            .len(),
        logged.len()
    );
    assert_eq!(
        last_message(&s.fx),
        DaemonMessage::TaskChanged { task: updated }
    );
}

#[test]
fn does_not_change_the_state_of_the_task() {
    let s = Setup::new();
    let task = s.started_task();
    s.later(60);
    s.activity(&task.id);
    assert_eq!(
        s.service.get_task(&task.id).unwrap().state,
        TaskState::Running
    );
}

#[test]
fn writes_nothing_again_before_the_interval_has_passed() {
    let s = Setup::new();
    let task = s.started_task();
    s.later(31);
    s.activity(&task.id);
    let count = s.fx.messages().len();

    s.later(10);

    assert!(!s.activity(&task.id));
    assert_eq!(s.updated_at(&task.id), "2026-10-08T09:00:31.000Z");
    assert_eq!(s.fx.messages().len(), count);
}

#[test]
fn uses_the_interval_it_is_given() {
    let s = Setup::new();
    let task = s.started_task();
    s.later(5);
    let every = |min_interval_ms| ActivityOptions {
        min_interval_ms,
        ..ActivityOptions::default()
    };

    assert!(!s.service.record_activity(&task.id, every(60_000)).unwrap());
    assert!(s.service.record_activity(&task.id, every(1000)).unwrap());
}

#[test]
fn ends_a_wait_for_background_work_at_once_unless_told_to_keep_it() {
    let s = Setup::new();
    let task = s.started_task();
    apply(
        &s.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.waiting" }),
    );
    let since = s.service.get_task(&task.id).unwrap().background_since;
    s.later(5);
    let keep = ActivityOptions {
        ends_wait: false,
        ..ActivityOptions::default()
    };

    assert!(!s.service.record_activity(&task.id, keep).unwrap());
    s.later(31);
    assert!(s.service.record_activity(&task.id, keep).unwrap());
    let kept = s.service.get_task(&task.id).unwrap();
    assert_eq!(kept.background_since, since);
    assert_eq!(kept.updated_at, "2026-10-08T09:00:36.000Z");

    s.later(1);
    assert!(s.activity(&task.id));
    assert_eq!(s.service.get_task(&task.id).unwrap().background_since, None);
}

#[test]
fn does_nothing_for_a_task_that_is_not_running() {
    for state in ["draft", "ready", "review"] {
        let s = Setup::new();
        let task = create(&s.service, "Fix it");
        override_to(&s.service, &task.id, state, None);
        s.later(3600);

        assert!(!s.activity(&task.id), "state {state}");
    }
}

#[test]
fn fails_with_not_found_for_an_unknown_task() {
    let s = Setup::new();
    assert!(matches!(
        s.service
            .record_activity("T-0099", ActivityOptions::default()),
        Err(TaskError::NotFound(_))
    ));
}

#[test]
fn writes_the_task_file_for_signs_of_life_once_a_minute_and_for_a_change_at_once() {
    let s = Setup::new();
    let task = s.started_task();
    let in_file = || file_task(&s.fx, &task.id).unwrap().updated_at;
    let index = s.fx.index();
    let started_at = in_file();

    s.later(31);
    assert!(s.activity(&task.id));
    assert_eq!(s.updated_at(&task.id), "2026-10-08T09:00:31.000Z");
    assert_eq!(
        index.list_tasks(None).unwrap()[0].updated_at,
        "2026-10-08T09:00:31.000Z"
    );
    assert_eq!(in_file(), started_at);

    s.later(31);
    s.activity(&task.id);
    assert_eq!(in_file(), "2026-10-08T09:01:02.000Z");

    s.later(31);
    s.activity(&task.id);
    assert_eq!(in_file(), "2026-10-08T09:01:02.000Z");

    s.later(1);
    apply(
        &s.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.waiting" }),
    );
    assert_eq!(in_file(), "2026-10-08T09:01:34.000Z");
}

#[test]
fn keeps_the_time_of_a_sign_of_life_that_is_not_in_the_file_yet_over_a_restart() {
    let s = Setup::new();
    let task = s.started_task();
    s.later(31);
    s.activity(&task.id);

    let restarted = s.fx.started(None);

    assert_eq!(
        restarted.get_task(&task.id).unwrap().updated_at,
        "2026-10-08T09:00:31.000Z"
    );
}

#[test]
fn rewrites_the_task_file_at_most_once_a_minute_for_a_sign_of_life_every_second() {
    let s = Setup::new();
    let task = s.started_task();
    let in_file = || file_task(&s.fx, &task.id).unwrap().updated_at;
    let index = s.fx.index();
    let mut file_times = vec![in_file()];
    let every_second = ActivityOptions {
        min_interval_ms: 1000,
        ..ActivityOptions::default()
    };

    for _ in 1..=180 {
        s.later(1);
        assert!(s.service.record_activity(&task.id, every_second).unwrap());
        let at = iso_from_millis(s.time.load(Ordering::SeqCst));
        assert_eq!(s.updated_at(&task.id), at);
        assert_eq!(index.list_tasks(None).unwrap()[0].updated_at, at);
        // A stop now would lose less than a minute, and the index has the rest.
        let age =
            s.time.load(Ordering::SeqCst) - natamux_core::js::parse_iso_millis(&in_file()).unwrap();
        assert!(age < 60_000);
        let seen = in_file();
        if !file_times.contains(&seen) {
            file_times.push(seen);
        }
    }

    assert_eq!(
        file_times,
        [
            "2026-10-08T09:00:00.000Z",
            "2026-10-08T09:01:00.000Z",
            "2026-10-08T09:02:00.000Z",
            "2026-10-08T09:03:00.000Z",
        ]
    );
}
