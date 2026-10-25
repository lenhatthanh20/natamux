//! The group 'init from the index of the last start'.

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{Duration, UNIX_EPOCH};

use natamux_store::event_log::event_log_path;
use natamux_store::task_files::{serialize_task, task_file_path, write_task_file};
use natamux_store::task_index::TaskIndex;

use super::*;

fn edit_by_hand(fx: &Fixture, task: &Task, title: &str) {
    let edited = Task {
        title: title.into(),
        ..task.clone()
    };
    fs::write(task_file_path(fx.data(), &task.id), serialize_task(&edited)).unwrap();
}

fn append_comment(fx: &Fixture, id: &str) {
    let line = json!({
        "taskId": id, "source": "human", "type": "review.commented",
        "at": "2026-10-08T10:00:00.000Z", "payload": { "text": "Looks good" }, "accepted": true
    });
    let mut log = OpenOptions::new()
        .append(true)
        .open(event_log_path(fx.data(), id))
        .unwrap();
    writeln!(log, "{line}").unwrap();
}

/// `utimesSync` in whole seconds, so a time set again is exactly the one the index saw.
fn set_mtime(path: &Path, seconds: u64) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap();
}

#[test]
fn takes_a_task_from_the_index_while_its_file_has_not_changed_and_reads_the_others() {
    let fx = Fixture::new();
    let kept = create(&fx.service, "Kept");
    let edited = create(&fx.service, "Edited");
    // What the index holds, to tell it from what the file holds.
    let mut index = fx.index();
    index
        .upsert_task(&Task {
            title: "From the index".into(),
            ..kept.clone()
        })
        .unwrap();
    index
        .upsert_task(&Task {
            title: "From the index".into(),
            ..edited.clone()
        })
        .unwrap();
    edit_by_hand(&fx, &edited, "Edited by hand");

    let restarted = fx.started(None);

    assert_eq!(
        restarted.get_task(&kept.id).unwrap().title,
        "From the index"
    );
    assert_eq!(
        restarted.get_task(&edited.id).unwrap().title,
        "Edited by hand"
    );
}

#[test]
fn reads_an_event_log_only_when_it_changed_since() {
    let fx = Fixture::new();
    let quiet = create(&fx.service, "Quiet");
    let busy = create(&fx.service, "Busy");
    // The index lost the events of both. It trusts a log that it matched last time.
    let mut index = fx.index();
    index.replace_events(&quiet.id, &[]).unwrap();
    index.replace_events(&busy.id, &[]).unwrap();
    append_comment(&fx, &busy.id);

    fx.started(None);

    assert_eq!(index.list_events(&quiet.id).unwrap(), []);
    assert_eq!(
        kinds(&index.list_events(&busy.id).unwrap()),
        ["task.created", "review.commented"]
    );
}

#[test]
fn loads_a_task_file_added_while_it_was_off_and_drops_one_that_was_removed() {
    let fx = Fixture::new();
    let gone = create(&fx.service, "Gone");
    fs::remove_file(task_file_path(fx.data(), &gone.id)).unwrap();
    write_task_file(
        fx.data(),
        &Task {
            id: "T-0005".into(),
            title: "Added".into(),
            ..gone
        },
    )
    .unwrap();

    let restarted = fx.started(None);

    assert_eq!(titles(&restarted.list_tasks(None).unwrap()), ["Added"]);
    assert_eq!(titles(&fx.index().list_tasks(None).unwrap()), ["Added"]);
}

#[test]
fn reads_a_task_file_again_when_only_its_size_or_only_its_time_changed() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    let path = task_file_path(fx.data(), &task.id);
    set_mtime(&path, 1_000_000);
    fx.started(None);

    edit_by_hand(&fx, &task, "Edited by hand");
    set_mtime(&path, 1_000_000);
    assert_eq!(
        fx.started(None).get_task(&task.id).unwrap().title,
        "Edited by hand"
    );

    edit_by_hand(&fx, &task, "Edited by HAND");
    set_mtime(&path, 2_000_000);
    assert_eq!(
        fx.started(None).get_task(&task.id).unwrap().title,
        "Edited by HAND"
    );
}

#[test]
fn ends_with_the_same_tasks_and_events_as_a_start_with_a_new_index() {
    let fx = Fixture::new();
    let kept = create(&fx.service, "Kept");
    let edited = create(&fx.service, "Edited");
    human(&fx.service, &kept.id, "task.readied");
    fx.started(None);
    edit_by_hand(&fx, &edited, "Edited by hand");
    append_comment(&fx, &kept.id);

    let rebuilt = fx.started_on(fx.memory_index(), None);
    let restarted = fx.started(None);

    assert_eq!(
        restarted.list_tasks(None).unwrap(),
        rebuilt.list_tasks(None).unwrap()
    );
    assert_eq!(index_tasks(&restarted), index_tasks(&rebuilt));
    for id in [&kept.id, &edited.id] {
        assert_eq!(
            event_pairs(&restarted.list_events(id).unwrap()),
            event_pairs(&rebuilt.list_events(id).unwrap())
        );
    }
}

#[test]
fn reads_every_file_with_an_index_from_before_the_file_stats() {
    let fx = Fixture::new();
    let task = create(&fx.service, "First");
    let path = fx.data().join("old.sqlite");
    let old = TaskIndex::open(&path).unwrap();
    // The old schema has no events and no file stats; opening the index adds them empty.
    let stale = serde_json::to_string(&Task {
        title: "Stale".into(),
        ..task.clone()
    })
    .unwrap();
    old.connection()
        .execute_batch(&format!(
            "drop table tasks_index; drop table events; drop table task_files;
             create table tasks_index (id text primary key, title text not null, state text not null,
               priority text not null, updated_at text not null, data text not null);
             insert into tasks_index values ('{}', 'Stale', '{}', '{}', '{}', '{}');",
            task.id,
            task.state.name(),
            task.priority,
            task.updated_at,
            stale.replace('\'', "''")
        ))
        .unwrap();
    drop(old);

    let upgraded = TaskIndex::open(&path).unwrap();
    let restarted = fx.started_on(upgraded, None);

    assert_eq!(restarted.get_task(&task.id).unwrap().title, "First");
    assert_eq!(titles(&index_tasks(&restarted)), ["First"]);
}
