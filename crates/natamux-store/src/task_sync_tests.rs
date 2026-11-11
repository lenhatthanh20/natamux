use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use super::*;

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
    sync.index
        .connection()
        .execute(
            "update tasks_index set priority = 'marked' where id = ?",
            [id],
        )
        .unwrap();
}

fn marked(sync: &TaskSync, id: &str) -> bool {
    let priority: String = sync
        .index
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
        .index
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
