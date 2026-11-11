use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use serde_json::json;

use super::*;
use crate::task_files::{serialize_task, write_task_file};

fn make_task(id: &str, title: &str) -> Task {
    Task {
        id: id.into(),
        title: title.into(),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn folder() -> PathBuf {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
        "natamux-sync-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn sync_in(dir: &Path) -> TaskSync {
    let index = TaskIndex::open(Path::new(":memory:")).unwrap();
    TaskSync::new(dir, index, Arc::new(|| 1_000), Box::new(|_| {}))
}

/// Marks a row: a full rewrite of the index drops it.
fn mark(sync: &TaskSync, id: &str) {
    sync.index()
        .connection()
        .execute(
            "update tasks_index set priority = 'marked' where id = ?",
            [id],
        )
        .unwrap();
}

fn marked(sync: &TaskSync, id: &str) -> bool {
    let priority: String = sync
        .index()
        .connection()
        .query_row(
            "select priority from tasks_index where id = ?",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    priority == "marked"
}

fn indexed_ids(sync: &TaskSync) -> Vec<String> {
    let mut ids: Vec<String> = sync
        .index()
        .list_tasks(None)
        .unwrap()
        .into_iter()
        .map(|task| task.id)
        .collect();
    ids.sort();
    ids
}

#[test]
fn a_second_init_with_unchanged_files_leaves_the_index_alone() {
    let dir = folder();
    write_task_file(&dir, &make_task("T-0001", "One")).unwrap();
    write_task_file(&dir, &make_task("T-0002", "Two")).unwrap();
    let mut sync = sync_in(&dir);
    sync.init().unwrap();
    mark(&sync, "T-0001");

    sync.init().unwrap();
    assert!(marked(&sync, "T-0001"));
    assert_eq!(sync.tasks.len(), 2);
    assert_eq!(indexed_ids(&sync), ["T-0001", "T-0002"]);
}

#[test]
fn an_init_after_a_change_gives_a_correct_index() {
    let dir = folder();
    write_task_file(&dir, &make_task("T-0001", "One")).unwrap();
    write_task_file(&dir, &make_task("T-0002", "Two")).unwrap();
    let mut sync = sync_in(&dir);
    sync.init().unwrap();

    // A file changed.
    write_task_file(&dir, &make_task("T-0001", "One, edited by hand")).unwrap();
    mark(&sync, "T-0002");
    sync.init().unwrap();
    assert!(!marked(&sync, "T-0002"));
    assert_eq!(sync.tasks["T-0001"].title, "One, edited by hand");

    // A file added.
    write_task_file(&dir, &make_task("T-0003", "Three")).unwrap();
    sync.init().unwrap();
    assert_eq!(indexed_ids(&sync), ["T-0001", "T-0002", "T-0003"]);

    // A file removed.
    std::fs::remove_file(task_file_path(&dir, "T-0002")).unwrap();
    sync.init().unwrap();
    assert_eq!(indexed_ids(&sync), ["T-0001", "T-0003"]);
    assert!(!sync.tasks.contains_key("T-0002"));
}

fn created(id: &str, at: &str) -> LoggedEvent {
    LoggedEvent {
        event: natamux_proto::events::task_event::TaskEvent {
            task_id: id.into(),
            source: "system".into(),
            kind: "task.created".into(),
            at: at.into(),
            payload: serde_json::Map::new(),
        },
        accepted: true,
    }
}

#[test]
fn writes_left_for_later_end_as_the_last_change_with_every_event_in_order() {
    let dir = folder();
    let mut sync = sync_in(&dir);
    let first = make_task("T-0001", "One");
    let second = make_task("T-0001", "Two");
    let (one, two) = ("2026-10-08T09:00:00.000Z", "2026-10-08T09:01:00.000Z");

    let (events_one, events_two) = ([created("T-0001", one)], [created("T-0001", two)]);

    sync.defer_writes();
    sync.store(first, &events_one, true).unwrap();
    sync.store(second.clone(), &events_two, true).unwrap();
    assert_eq!(sync.tasks["T-0001"], second);
    assert!(read_task_file(&dir, "T-0001").unwrap().is_none());
    let writes = sync.take_writes().unwrap();
    assert!(sync.take_writes().is_none());
    assert!(writes.run().result.is_ok());

    assert_eq!(
        read_task_file(&dir, "T-0001").unwrap(),
        Some(second.clone())
    );
    let ats: Vec<String> = sync
        .list_events("T-0001")
        .unwrap()
        .into_iter()
        .map(|stored| stored.event.at)
        .collect();
    assert_eq!(ats, [one, two]);
    assert_eq!(read_event_log(&dir, "T-0001").unwrap().unwrap().len(), 2);
    assert_eq!(sync.index().list_tasks(None).unwrap(), [second]);
}

#[test]
fn a_write_left_for_later_that_fails_takes_the_change_back_from_memory() {
    let dir = folder();
    let mut sync = sync_in(&dir);
    let first = make_task("T-0001", "One");
    sync.store(first.clone(), &[], true).unwrap();
    let mut temporary = task_file_path(&dir, "T-0001").into_os_string();
    temporary.push(".tmp");
    std::fs::create_dir(temporary).unwrap();

    sync.defer_writes();
    sync.store(make_task("T-0001", "Two"), &[], true).unwrap();
    let ran = sync.take_writes().unwrap().run();
    assert!(ran.result.is_err());
    for task_id in &ran.failed {
        sync.restore(task_id);
    }

    assert_eq!(sync.tasks["T-0001"], first);
    assert_eq!(read_task_file(&dir, "T-0001").unwrap(), Some(first.clone()));
    assert_eq!(sync.index().list_tasks(None).unwrap(), [first]);
}

#[test]
fn a_write_whose_index_fails_keeps_the_change_its_file_holds() {
    let dir = folder();
    let mut sync = sync_in(&dir);
    let refuse = "create trigger refuse before insert on events begin
        select raise(abort, 'refused'); end;";
    sync.index().connection().execute_batch(refuse).unwrap();
    let task = make_task("T-0001", "One");
    let events = [created("T-0001", "2026-10-08T09:00:00.000Z")];

    sync.defer_writes();
    sync.store(task.clone(), &events, true).unwrap();
    let ran = sync.take_writes().unwrap().run();

    assert!(ran.result.is_err());
    for task_id in &ran.failed {
        sync.restore(task_id);
    }

    // As a write under the lock: the file has the change, so memory keeps it, and the next
    // start reads the file again, as its stat is not the one the index holds.
    assert_eq!(sync.tasks["T-0001"], task);
    assert_eq!(read_task_file(&dir, "T-0001").unwrap(), Some(task));
    assert_eq!(sync.list_events("T-0001").unwrap(), []);
    assert_eq!(sync.index().list_tasks(None).unwrap(), []);
}

#[test]
fn two_failed_writes_in_a_row_leave_the_task_as_last_stored() {
    let dir = folder();
    let mut sync = sync_in(&dir);
    let stored = make_task("T-0001", "One");
    sync.store(stored.clone(), &[], true).unwrap();
    // Folders where the temporary task file and the event log go: both writes fail.
    let mut temporary = task_file_path(&dir, "T-0001").into_os_string();
    temporary.push(".tmp");
    std::fs::create_dir(temporary).unwrap();
    std::fs::create_dir(event_log_path(&dir, "T-0001")).unwrap();
    let at = "2026-10-08T09:01:00.000Z";
    let events = [created("T-0001", at)];

    sync.defer_writes();
    sync.store(make_task("T-0001", "Two"), &[], true).unwrap();
    let first = sync.take_writes().unwrap();
    sync.defer_writes();
    sync.store(make_task("T-0001", "Three"), &events, false)
        .unwrap();
    let second = sync.take_writes().unwrap();
    let ran = [first.run(), second.run()];
    // The first restore may come before the second; both end with the stored task.
    for task_id in ran.iter().flat_map(|ran| &ran.failed) {
        sync.restore(task_id);
    }

    assert!(ran.iter().all(|ran| ran.result.is_err()));
    assert_eq!(sync.tasks["T-0001"], stored);
    assert!(!sync.written_at.contains_key("T-0001"));
    assert_eq!(
        read_task_file(&dir, "T-0001").unwrap(),
        Some(stored.clone())
    );
    assert_eq!(sync.index().list_tasks(None).unwrap(), [stored]);
}

#[test]
fn a_reader_of_events_on_another_thread_waits_for_a_pending_write() {
    let dir = folder();
    let sync = Arc::new(std::sync::Mutex::new(sync_in(&dir)));
    let events = [created("T-0001", "2026-10-08T09:00:00.000Z")];
    let writes = {
        let mut sync = sync.lock().unwrap();
        sync.defer_writes();
        sync.store(make_task("T-0001", "One"), &events, true)
            .unwrap();
        sync.take_writes().unwrap()
    };
    let read = Arc::new(AtomicBool::new(false));
    let reader = {
        let (sync, read) = (Arc::clone(&sync), Arc::clone(&read));
        std::thread::spawn(move || {
            let count = sync.lock().unwrap().list_events("T-0001").unwrap().len();
            read.store(true, Ordering::SeqCst);
            count
        })
    };

    std::thread::sleep(Duration::from_millis(50));
    assert!(!read.load(Ordering::SeqCst));
    assert!(writes.run().result.is_ok());

    assert_eq!(reader.join().unwrap(), 1);
}

#[test]
fn a_check_during_a_pending_write_neither_waits_nor_reads_the_file_back() {
    let dir = folder();
    let reloads = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&reloads);
    let index = TaskIndex::open(Path::new(":memory:")).unwrap();
    let on_changed = Box::new(move |_: &Task| {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    let mut sync = TaskSync::new(&dir, index, Arc::new(|| 1_000), on_changed);
    sync.store(make_task("T-0001", "One"), &[], true).unwrap();
    sync.init().unwrap();

    sync.defer_writes();
    sync.store(make_task("T-0001", "Two"), &[], true).unwrap();
    let writes = sync.take_writes().unwrap();
    // The write is pending: a check would wait for ever if it waited.
    sync.try_reload("T-0001");
    assert!(writes.run().result.is_ok());
    sync.try_reload("T-0001");
    assert_eq!(reloads.load(Ordering::SeqCst), 0);

    let by_hand = make_task("T-0001", "Edited by hand");
    std::fs::write(task_file_path(&dir, "T-0001"), serialize_task(&by_hand)).unwrap();
    sync.try_reload("T-0001");
    assert_eq!(reloads.load(Ordering::SeqCst), 1);
    assert_eq!(sync.tasks["T-0001"].title, "Edited by hand");
}

#[test]
fn a_delete_waits_for_a_pending_write_so_no_file_comes_back() {
    let dir = folder();
    let sync = Arc::new(std::sync::Mutex::new(sync_in(&dir)));
    let events = [created("T-0001", "2026-10-08T09:00:00.000Z")];
    let writes = {
        let mut sync = sync.lock().unwrap();
        sync.defer_writes();
        sync.store(make_task("T-0001", "One"), &events, true)
            .unwrap();
        sync.take_writes().unwrap()
    };
    // As `remove_stored_task`: settle, remove the files and the row, forget.
    let deleter = {
        let (sync, dir) = (Arc::clone(&sync), dir.clone());
        std::thread::spawn(move || {
            let mut sync = sync.lock().unwrap();
            sync.settle("T-0001");
            std::fs::remove_file(task_file_path(&dir, "T-0001")).unwrap();
            std::fs::remove_file(event_log_path(&dir, "T-0001")).unwrap();
            sync.index().delete_task("T-0001").unwrap();
            sync.forget("T-0001");
        })
    };

    std::thread::sleep(Duration::from_millis(50));
    assert!(!deleter.is_finished());
    assert!(writes.run().result.is_ok());
    deleter.join().unwrap();

    let sync = sync.lock().unwrap();
    assert!(!task_file_path(&dir, "T-0001").exists());
    assert!(!event_log_path(&dir, "T-0001").exists());
    assert_eq!(sync.index().list_tasks(None).unwrap(), []);
    assert_eq!(sync.index().count_events("T-0001").unwrap(), 0);
    assert!(!sync.tasks.contains_key("T-0001"));
}

#[test]
fn a_restore_keeps_a_newer_row_over_a_file_whose_row_failed() {
    let dir = folder();
    let mut sync = sync_in(&dir);
    sync.store(make_task("T-0001", "One"), &[], true).unwrap();
    // Only a change with an event fails in the index.
    let refuse = "create trigger refuse before insert on events begin
        select raise(abort, 'refused'); end;";
    sync.index().connection().execute_batch(refuse).unwrap();
    let renamed = make_task("T-0001", "Two");
    let events = [created("T-0001", "2026-10-08T09:01:00.000Z")];
    let later = Task {
        updated_at: "2026-10-08T09:02:00.000Z".into(),
        ..renamed.clone()
    };

    sync.defer_writes();
    sync.store(renamed, &events, true).unwrap();
    let file_only = sync.take_writes().unwrap();
    // A sign of life: only the row, built on the change before it.
    sync.defer_writes();
    sync.store(later.clone(), &[], false).unwrap();
    let row_only = sync.take_writes().unwrap();
    let failed = file_only.run().failed;
    assert!(row_only.run().result.is_ok());
    for task_id in &failed {
        sync.restore(task_id);
    }

    assert_eq!(sync.tasks["T-0001"], later);
    assert_eq!(sync.index().list_tasks(None).unwrap(), [later]);
}

#[test]
fn a_failed_event_line_keeps_the_change_its_file_holds_and_loses_the_event() {
    let dir = folder();
    let mut sync = sync_in(&dir);
    sync.store(make_task("T-0001", "One"), &[], true).unwrap();
    std::fs::create_dir(event_log_path(&dir, "T-0001")).unwrap();
    let renamed = make_task("T-0001", "Two");
    let events = [created("T-0001", "2026-10-08T09:01:00.000Z")];

    sync.defer_writes();
    sync.store(renamed.clone(), &events, true).unwrap();
    let ran = sync.take_writes().unwrap().run();
    assert!(ran.result.is_err());
    for task_id in &ran.failed {
        sync.restore(task_id);
    }

    // On purpose: memory follows the file, which has the change; the caller still hears of
    // the failure, and the event is in no log and no row, as under the lock.
    assert_eq!(sync.tasks["T-0001"], renamed);
    assert_eq!(read_task_file(&dir, "T-0001").unwrap(), Some(renamed));
    assert_eq!(sync.list_events("T-0001").unwrap(), []);
}
