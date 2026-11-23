use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::json;

use super::*;

const MARK: &str = "PROMPT-MARK-4711";

fn folder() -> PathBuf {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let folder = std::env::temp_dir().join(format!(
        "natamux-index-privacy-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

fn task(id: &str) -> Task {
    Task {
        id: id.into(),
        title: format!("Title {MARK}"),
        description: format!("Notes {MARK}"),
        priority: "normal".into(),
        stage: "implement".into(),
        stages: vec![json!("implement")],
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:00:00.000Z".into(),
        ..Task::default()
    }
}

fn holds(path: &Path, text: &str) -> bool {
    let bytes = fs::read(path).unwrap_or_default();
    bytes
        .windows(text.len())
        .any(|window| window == text.as_bytes())
}

#[test]
fn secure_delete_is_on() {
    let folder = folder();
    let index = TaskIndex::open(&folder.join("index.sqlite")).unwrap();
    let on: i64 = index
        .connection()
        .pragma_query_value(None, "secure_delete", |row| row.get(0))
        .unwrap();
    assert_eq!(on, 1);
    drop(index);
    let _ = fs::remove_dir_all(&folder);
}

#[test]
fn a_deleted_task_leaves_no_bytes_in_the_index_or_its_wal() {
    let folder = folder();
    let path = folder.join("index.sqlite");
    let mut index = TaskIndex::open(&path).unwrap();
    index.upsert_task(&task("T-0001")).unwrap();
    index.upsert_task(&task("T-0002")).unwrap();
    index.delete_task("T-0001").unwrap();
    index.delete_task("T-0002").unwrap();

    assert_eq!(index.get_task("T-0001").unwrap(), None);
    let wal = folder.join("index.sqlite-wal");
    assert_eq!(fs::metadata(&wal).map(|meta| meta.len()).unwrap_or(0), 0);
    assert!(!holds(&path, MARK));
    assert!(!holds(&path, "T-0001"));
    drop(index);
    let _ = fs::remove_dir_all(&folder);
}

#[cfg(unix)]
#[test]
fn the_index_and_its_wal_files_are_only_the_users() {
    use std::os::unix::fs::PermissionsExt;
    let folder = folder();
    let path = folder.join("index.sqlite");
    fs::write(&path, b"").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let index = TaskIndex::open(&path).unwrap();
    for name in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
        let mode = fs::metadata(folder.join(name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{name}");
    }
    drop(index);
    let _ = fs::remove_dir_all(&folder);
}
