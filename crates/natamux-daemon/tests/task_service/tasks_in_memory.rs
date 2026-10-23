//! The group 'tasks in memory'.

use natamux_daemon::tasks::errors::TaskError;
use natamux_proto::tasks::task::TaskState;
use natamux_store::task_files::{task_file_path, tasks_dir};

use super::*;

#[test]
fn reads_no_task_file_for_get_task_and_list_tasks_after_init() {
    let fx = Fixture::new();
    let first = create(&fx.service, "First");
    let second = create(&fx.service, "Second");
    let restarted = fx.started(None);

    fs::remove_dir_all(tasks_dir(fx.data())).unwrap();

    assert_eq!(restarted.get_task(&first.id).unwrap(), first);
    assert_eq!(
        ids(&restarted.list_tasks(None).unwrap()),
        [second.id, first.id]
    );
    assert!(matches!(
        restarted.get_task("T-0099"),
        Err(TaskError::NotFound(_))
    ));
}

#[test]
fn lists_the_newest_updated_task_first_as_the_index_did() {
    let fx = Fixture::new();
    let first = create(&fx.service, "First");
    let second = create(&fx.service, "Second");
    human(&fx.service, &first.id, "task.readied");

    assert_eq!(
        ids(&fx.service.list_tasks(None).unwrap()),
        [first.id.clone(), second.id.clone()]
    );
    assert_eq!(
        ids(&fx.service.list_tasks(Some(TaskState::Draft)).unwrap()),
        [second.id]
    );
}

#[test]
fn keeps_the_task_it_had_before_a_change_as_it_was() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    let before = fx.service.get_task(&task.id).unwrap();

    fx.service
        .update_task(&task.id, Some(&json!({ "title": "Renamed" })))
        .unwrap();

    assert_eq!(before.title, "First");
    assert_eq!(fx.service.get_task(&task.id).unwrap().title, "Renamed");
}

#[test]
fn forgets_a_deleted_task() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    fx.service.delete_task(&task.id).unwrap();

    assert!(matches!(
        fx.service.get_task(&task.id),
        Err(TaskError::NotFound(_))
    ));
    assert_eq!(fx.service.list_tasks(None).unwrap(), []);
}

// Node froze the tasks it gave. A Rust caller owns a copy, so the check is that changing the copy
// leaves the service as it was.
#[test]
fn gives_tasks_that_a_caller_cannot_change() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    let restarted = fx.started(None);
    let mut given = vec![
        task,
        fx.service.get_task("T-0001").unwrap(),
        restarted.get_task("T-0001").unwrap(),
        fx.make(None).get_task("T-0001").unwrap(),
    ];
    given.extend(restarted.list_tasks(None).unwrap());
    assert_eq!(given.len(), 5);

    for each in &mut given {
        each.title = "Changed".into();
    }

    assert_eq!(fx.service.get_task("T-0001").unwrap().title, "First");
    assert_eq!(restarted.get_task("T-0001").unwrap().title, "First");
}

#[test]
fn keeps_the_task_as_it_was_when_its_file_cannot_be_written() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    // A folder where the temporary file goes makes the write fail on every platform.
    let mut temporary = task_file_path(fx.data(), &task.id).into_os_string();
    temporary.push(".tmp");
    fs::create_dir(temporary).unwrap();

    assert!(
        fx.service
            .update_task(&task.id, Some(&json!({ "title": "Renamed" })))
            .is_err()
    );

    assert_eq!(fx.service.get_task(&task.id).unwrap(), task);
    assert_eq!(
        fx.index().list_tasks(None).unwrap(),
        std::slice::from_ref(&task)
    );
    assert_eq!(event_kinds(&fx.service, &task.id), ["task.created"]);
}

// Node counted `begin` and `commit` through a spy on the SQLite handle. That cannot be ported, so
// this checks the result of the two stores: each event is in the index with its flag.
#[test]
fn stores_an_accepted_and_a_rejected_event() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");

    let rejected = apply(
        &fx.service,
        &task.id,
        json!({ "source": "agent", "type": "agent.finished" }),
    );
    assert!(!rejected.accepted);
    human(&fx.service, &task.id, "task.readied");

    let flags: Vec<bool> = fx
        .index()
        .list_events(&task.id)
        .unwrap()
        .iter()
        .map(|each| each.accepted)
        .collect();
    assert_eq!(flags, [true, false, true]);
    let listed: Vec<bool> = fx
        .service
        .list_events(&task.id)
        .unwrap()
        .iter()
        .map(|each| each.accepted)
        .collect();
    assert_eq!(listed, [true, false, true]);
}
