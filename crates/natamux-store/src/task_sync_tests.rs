use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use super::*;
use crate::task_files::write_task_file;

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
    for undo in ran.undo {
        sync.undo(undo);
    }

    assert_eq!(sync.tasks["T-0001"], first);
    assert_eq!(read_task_file(&dir, "T-0001").unwrap(), Some(first.clone()));
    assert_eq!(sync.index().list_tasks(None).unwrap(), [first]);
}
