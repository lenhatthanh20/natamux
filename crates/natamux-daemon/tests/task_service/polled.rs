//! The group 'outside edits without a watch event': the service starts with the watcher off, as
//! on a file system that gives no events. Node's `console.error` spy cannot be ported; where a
//! test counted the log lines it checks the task it kept instead.

use std::sync::atomic::{AtomicI64, Ordering};

use natamux_daemon::tasks::errors::TaskError;
use natamux_store::event_log::read_event_log;
use natamux_store::task_files::{serialize_task, task_file_path, tasks_dir};

use super::*;

struct Setup {
    fx: Fixture,
    time: Arc<AtomicI64>,
    polled: TaskService,
}

impl Setup {
    fn new() -> Setup {
        let fx = Fixture::new();
        let (time, clock) = manual_clock(0);
        let polled = fx.with(Build {
            clock: Some(clock),
            ..Build::default()
        });
        Setup { fx, time, polled }
    }

    fn at(&self, millis: i64) {
        self.time.store(millis, Ordering::SeqCst);
    }

    fn start(&self) {
        self.polled.init().unwrap();
        self.polled.close();
        self.fx.clear_messages();
    }

    fn edit_by_hand(&self, task: &Task, title: &str) {
        let edited = Task {
            title: title.into(),
            ..task.clone()
        };
        fs::write(
            task_file_path(self.fx.data(), &task.id),
            serialize_task(&edited),
        )
        .unwrap();
    }

    fn title(&self, id: &str) -> String {
        self.polled.get_task(id).unwrap().title
    }
}

fn changed(task: Task) -> DaemonMessage {
    DaemonMessage::TaskChanged { task }
}

#[test]
fn sees_an_edit_at_the_first_get_task_a_second_after_the_last_look_not_before() {
    let s = Setup::new();
    let task = create(&s.fx.service, "First");
    s.start();
    s.edit_by_hand(&task, "Edited by hand");

    s.at(999);
    assert_eq!(s.title(&task.id), "First");

    s.at(1000);
    assert_eq!(s.title(&task.id), "Edited by hand");
    assert_eq!(
        s.fx.messages(),
        [changed(s.polled.get_task(&task.id).unwrap())]
    );

    // The next look is a second after this one.
    s.edit_by_hand(&task, "Edited again");
    s.at(1999);
    assert_eq!(s.title(&task.id), "Edited by hand");
    s.at(2000);
    assert_eq!(s.title(&task.id), "Edited again");
}

#[test]
fn finds_a_new_file_and_an_edit_at_list_tasks_every_two_seconds_and_keeps_a_removed_one() {
    let s = Setup::new();
    let edited = create(&s.fx.service, "First");
    let removed = create(&s.fx.service, "Second");
    s.start();
    s.edit_by_hand(&edited, "Edited by hand");
    let added = Task {
        id: "T-0007".into(),
        title: "Added by hand".into(),
        ..edited.clone()
    };
    fs::write(
        task_file_path(s.fx.data(), "T-0007"),
        serialize_task(&added),
    )
    .unwrap();
    fs::remove_file(task_file_path(s.fx.data(), &removed.id)).unwrap();
    let sorted_titles = || {
        let mut found = titles(&s.polled.list_tasks(None).unwrap());
        found.sort();
        found
    };

    s.at(1999);
    assert_eq!(sorted_titles(), ["First", "Second"]);

    s.at(2000);
    assert_eq!(
        sorted_titles(),
        ["Added by hand", "Edited by hand", "Second"]
    );
    assert_eq!(s.title("T-0007"), "Added by hand");
    let messages = s.fx.messages();
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, DaemonMessage::TaskCreated { .. }))
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, DaemonMessage::TaskDeleted { .. }))
    );
}

#[test]
fn does_not_read_back_its_own_writes() {
    let s = Setup::new();
    let task = create(&s.fx.service, "First");
    s.start();

    s.at(500);
    s.polled
        .update_task(&task.id, Some(&json!({ "title": "Renamed" })))
        .unwrap();
    let own = create(&s.polled, "Own");
    s.fx.clear_messages();
    s.at(10_000);
    s.polled.list_tasks(None).unwrap();
    s.polled.get_task(&task.id).unwrap();
    s.polled.get_task(&own.id).unwrap();

    assert_eq!(s.fx.messages(), []);
    assert_eq!(s.title(&task.id), "Renamed");
}

#[test]
fn does_not_write_over_an_edit_by_hand_it_has_not_read_yet_and_reads_it_instead() {
    let s = Setup::new();
    let task = create(&s.fx.service, "First");
    s.start();
    s.edit_by_hand(&task, "Edited by hand");

    // Within a second of the last look, so get_task still has the task from before the edit.
    s.at(500);
    let result = s
        .polled
        .update_task(&task.id, Some(&json!({ "priority": "high" })));
    assert!(matches!(result, Err(TaskError::FileChanged(_))));

    assert_eq!(file_task(&s.fx, &task.id).unwrap().title, "Edited by hand");
    assert_eq!(s.title(&task.id), "Edited by hand");
    assert_eq!(
        s.fx.messages(),
        [changed(s.polled.get_task(&task.id).unwrap())]
    );
    assert_eq!(event_kinds(&s.polled, &task.id), ["task.created"]);
    let log = read_event_log(s.fx.data(), &task.id).unwrap().unwrap();
    assert_eq!(
        log.iter()
            .map(|each| each.event.kind.as_str())
            .collect::<Vec<_>>(),
        ["task.created"]
    );

    // Sent again, the change keeps the edit.
    s.polled
        .update_task(&task.id, Some(&json!({ "priority": "high" })))
        .unwrap();
    let stored = file_task(&s.fx, &task.id).unwrap();
    assert_eq!(
        (stored.title.as_str(), stored.priority.as_str()),
        ("Edited by hand", "high")
    );
}

#[test]
fn keeps_the_last_good_task_while_its_file_cannot_be_read() {
    let s = Setup::new();
    let task = create(&s.fx.service, "First");
    s.start();
    fs::write(
        task_file_path(s.fx.data(), &task.id),
        "---\nid: nope\n---\n",
    )
    .unwrap();

    s.at(1000);
    assert_eq!(s.polled.get_task(&task.id).unwrap(), task);
    s.at(5000);
    s.polled.list_tasks(None).unwrap();
    assert_eq!(s.polled.get_task(&task.id).unwrap(), task);
}

#[test]
fn still_lists_the_tasks_when_the_folder_cannot_be_read() {
    let s = Setup::new();
    let task = create(&s.fx.service, "First");
    s.start();
    // A file in place of the folder: read_dir fails with ENOTDIR, not NotFound.
    fs::remove_dir_all(tasks_dir(s.fx.data())).unwrap();
    fs::write(tasks_dir(s.fx.data()), "").unwrap();

    s.at(2000);
    assert_eq!(s.polled.list_tasks(None).unwrap(), [task]);
}
