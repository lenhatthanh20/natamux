//! The group 'a task file that is broken at the start'. Node's `console.error` spy cannot be
//! ported; the tests check which tasks the service has instead.

use std::sync::atomic::Ordering;

use natamux_daemon::tasks::errors::TaskError;
use natamux_store::task_files::{serialize_task, task_file_path};

use super::*;

const BROKEN: &str = "---\ntitle: [unclosed\n---\n";

fn fresh(fx: &Fixture, clock: Clock) -> TaskService {
    fx.with(Build {
        clock: Some(clock),
        ..Build::default()
    })
}

#[test]
fn starts_with_the_other_tasks_and_leaves_out_a_broken_task_the_index_never_saw() {
    let fx = Fixture::new();
    let (_, clock) = manual_clock(0);
    let fresh = fresh(&fx, clock);
    let good = create(&fx.service, "Good");
    fs::write(task_file_path(fx.data(), "T-0009"), BROKEN).unwrap();

    fresh.init().unwrap();

    assert_eq!(ids(&fresh.list_tasks(None).unwrap()), [good.id]);
    assert!(matches!(
        fresh.get_task("T-0009"),
        Err(TaskError::NotFound(_))
    ));
    fresh.close();
}

#[test]
fn keeps_the_task_as_the_index_saw_it_last() {
    let fx = Fixture::new();
    let (_, clock) = manual_clock(0);
    let fresh = fresh(&fx, clock);
    let task = create(&fx.service, "Last good");
    fs::write(task_file_path(fx.data(), &task.id), BROKEN).unwrap();

    fresh.init().unwrap();

    assert_eq!(fresh.get_task(&task.id).unwrap().title, "Last good");
    fresh.close();
}

#[test]
fn reads_the_file_again_once_it_is_fixed_and_not_before() {
    let fx = Fixture::new();
    let (time, clock) = manual_clock(0);
    let fresh = fresh(&fx, clock);
    let task = create(&fx.service, "First");
    fs::write(task_file_path(fx.data(), "T-0009"), BROKEN).unwrap();
    fresh.init().unwrap();

    time.store(2000, Ordering::SeqCst);
    assert_eq!(
        ids(&fresh.list_tasks(None).unwrap()),
        std::slice::from_ref(&task.id)
    );

    let fixed = Task {
        id: "T-0009".into(),
        title: "Fixed by hand".into(),
        ..task
    };
    fs::write(task_file_path(fx.data(), "T-0009"), serialize_task(&fixed)).unwrap();
    time.store(4000, Ordering::SeqCst);
    assert!(titles(&fresh.list_tasks(None).unwrap()).contains(&"Fixed by hand".to_string()));
    assert_eq!(fresh.get_task("T-0009").unwrap().title, "Fixed by hand");
    fresh.close();
}
