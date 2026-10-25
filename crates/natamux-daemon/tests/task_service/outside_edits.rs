//! The group 'task files edited outside Natamux': the real file watcher. macOS may drop the events
//! of writes made right after `init`, before its watch starts. Then `list_tasks` finds the files,
//! two seconds after `init` at most; `watch_task_files` has its own test.

use std::thread::sleep;
use std::time::{Duration, Instant};

use natamux_store::task_files::{serialize_task, task_file_path};

use super::*;

const SEEN: Duration = Duration::from_secs(4);

fn eventually(watching: &TaskService, check: impl Fn() -> bool) {
    let until = Instant::now() + SEEN;
    loop {
        watching.list_tasks(None).unwrap();
        if check() {
            return;
        }
        assert!(Instant::now() < until, "the edit was not seen in {SEEN:?}");
        sleep(Duration::from_millis(20));
    }
}

fn write_task(fx: &Fixture, task: &Task) {
    fs::write(task_file_path(fx.data(), &task.id), serialize_task(task)).unwrap();
}

fn changed(task: Task) -> DaemonMessage {
    DaemonMessage::TaskChanged { task }
}

#[test]
fn sees_a_task_file_edited_or_added_and_keeps_one_removed() {
    let fx = Fixture::new();
    let watching = fx.make(None);
    let edited = create(&watching, "First");
    let removed = create(&watching, "Second");
    watching.init().unwrap();
    fx.clear_messages();

    write_task(
        &fx,
        &Task {
            title: "Edited by hand".into(),
            ..edited.clone()
        },
    );
    write_task(
        &fx,
        &Task {
            id: "T-0007".into(),
            title: "Added by hand".into(),
            ..edited.clone()
        },
    );
    fs::remove_file(task_file_path(fx.data(), &removed.id)).unwrap();

    eventually(&watching, || {
        let title = |id: &str| {
            watching
                .get_task(id)
                .map(|task| task.title)
                .unwrap_or_default()
        };
        title(&edited.id) == "Edited by hand" && title("T-0007") == "Added by hand"
    });
    assert_eq!(watching.get_task(&removed.id).unwrap().title, "Second");
    let messages = fx.messages();
    assert!(messages.contains(&changed(watching.get_task(&edited.id).unwrap())));
    // A new file does not count as a new task: task.created would start its context stage.
    assert!(messages.contains(&changed(watching.get_task("T-0007").unwrap())));
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, DaemonMessage::TaskCreated { .. }))
    );
    // task.deleted would end the task's agents.
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, DaemonMessage::TaskDeleted { .. }))
    );
}

#[test]
fn does_not_read_back_its_own_writes() {
    let fx = Fixture::new();
    let watching = fx.make(None);
    watching.init().unwrap();
    let own = create(&watching, "Own");
    watching
        .update_task(&own.id, Some(&json!({ "title": "Own, renamed" })))
        .unwrap();
    human(&watching, &own.id, "task.readied");
    write_task(
        &fx,
        &Task {
            id: "T-0009".into(),
            title: "Marker".into(),
            ..own
        },
    );

    eventually(&watching, || {
        watching
            .get_task("T-0009")
            .is_ok_and(|task| task.title == "Marker")
    });
    let seen: Vec<DaemonMessage> = fx
        .messages()
        .into_iter()
        .filter(|message| matches!(message, DaemonMessage::TaskChanged { .. }))
        .collect();
    assert_eq!(seen, [changed(watching.get_task("T-0009").unwrap())]);
}

// Node also checked that `console.error` named the task; stderr is not captured here.
#[test]
fn keeps_the_last_good_task_while_its_file_cannot_be_read() {
    let fx = Fixture::new();
    let watching = fx.make(None);
    let task = create(&watching, "First");
    watching.init().unwrap();

    fs::write(task_file_path(fx.data(), &task.id), "---\nid: nope\n---\n").unwrap();
    write_task(
        &fx,
        &Task {
            id: "T-0009".into(),
            title: "Marker".into(),
            ..task.clone()
        },
    );

    eventually(&watching, || {
        watching
            .get_task("T-0009")
            .is_ok_and(|each| each.title == "Marker")
    });
    assert_eq!(watching.get_task(&task.id).unwrap(), task);
}
