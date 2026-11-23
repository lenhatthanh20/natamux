//! A deleted task leaves nothing in the data folder: no path holds its id and no file holds its
//! id or its text. A delete cut short is finished at the next start.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::tasks::purge::{deleting_marker, purge_task_files, sweep_orphans};
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::terminals::screen_files::{save_screen, screens_dir};
use natamux_daemon::terminals::shell_files::{OpenShell, save_open_shells};
use natamux_store::attachments::AttachmentFile;
use natamux_store::task_files::{task_file_path, tasks_dir};
use natamux_store::task_index::TaskIndex;
use serde_json::json;

const MARK: &str = "PROMPT-MARK";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "natamux-delete-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn index_of(data: &Path) -> TaskIndex {
    TaskIndex::open(&data.join("index.sqlite")).unwrap()
}

fn service(data: &Path) -> TaskService {
    TaskService::new(TaskServiceOptions {
        data_dir: data.to_path_buf(),
        index: index_of(data),
        bus: EventBus::new(),
        now: None,
        settings_of: None,
        clock: None,
    })
}

/// A task with every kind of file Natamux keeps for it, each one holding the marker.
fn full_task(data: &Path, tasks: &TaskService) -> String {
    let attachment = AttachmentFile {
        name: "notes.txt".into(),
        bytes: format!("attached {MARK}").into_bytes(),
    };
    let input = json!({ "title": "Secret work", "description": format!("Do it {MARK}") });
    let id = tasks
        .create_task(Some(&input), &[attachment], true)
        .unwrap()
        .id;
    save_screen(data, &id, "1", format!("$ {MARK}").as_bytes()).unwrap();
    let shell = OpenShell {
        name: "1".into(),
        agent: Some("claude-code".into()),
        session: Some(MARK.into()),
        paused: false,
    };
    save_open_shells(data, &id, &[shell]).unwrap();
    let tasks_folder = tasks_dir(data);
    fs::write(tasks_folder.join(format!("{id}.shells.json")), MARK).unwrap();
    fs::write(tasks_folder.join(format!("{id}.usage.json")), MARK).unwrap();
    for script in [
        format!("agent-launch-{id}.sh"),
        format!("agent-launch-{id}-2.sh"),
        format!("stage-command-{id}.sh"),
    ] {
        fs::write(data.join(script), format!("echo {MARK}")).unwrap();
    }
    id
}

fn files_under(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        found.push(path.clone());
        if path.is_dir() {
            found.extend(files_under(&path));
        }
    }
    found.sort();
    found
}

/// Every path and file under the data folder that holds one of `texts`.
fn holders(data: &Path, texts: &[&str]) -> Vec<String> {
    let mut found = Vec::new();
    for path in files_under(data) {
        let name = path.to_string_lossy().into_owned();
        let bytes = if path.is_file() {
            fs::read(&path).unwrap()
        } else {
            Vec::new()
        };
        for text in texts {
            let in_bytes = bytes
                .windows(text.len())
                .any(|window| window == text.as_bytes());
            if name.contains(text) || in_bytes {
                found.push(format!("{name}: {text}"));
            }
        }
    }
    found
}

fn snapshot(data: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    files_under(data)
        .into_iter()
        // Readers write their marks into the shared-memory file; it holds no task data.
        .filter(|path| path.is_file() && !path.to_string_lossy().ends_with("-shm"))
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect()
}

#[test]
fn a_delete_leaves_no_path_or_byte_of_the_task() {
    let dir = TempDir::new();
    let data = dir.0.as_path();
    let tasks = service(data);
    let id = full_task(data, &tasks);
    let kept = tasks
        .create_task(Some(&json!({ "title": "Other" })), &[], true)
        .unwrap();
    assert!(screens_dir(data, &id).join("1.ans").is_file());
    assert!(!holders(data, &[id.as_str(), MARK]).is_empty());

    tasks.delete_task(&id).unwrap();

    assert_eq!(holders(data, &[id.as_str(), MARK]), Vec::<String>::new());
    let index = index_of(data);
    assert_eq!(index.get_task(&id).unwrap(), None);
    assert_eq!(index.count_events(&id).unwrap(), 0);
    assert!(task_file_path(data, &kept.id).is_file());
    assert!(index.get_task(&kept.id).unwrap().is_some());
}

#[test]
fn a_second_delete_changes_nothing() {
    let dir = TempDir::new();
    let data = dir.0.as_path();
    let tasks = service(data);
    let id = full_task(data, &tasks);
    tasks.delete_task(&id).unwrap();
    let before = snapshot(data);

    assert!(tasks.delete_task(&id).is_err());
    purge_task_files(data, &id).unwrap();

    assert_eq!(snapshot(data), before);
}

#[test]
fn the_start_sweep_finishes_a_delete_that_was_cut_short_and_then_does_nothing() {
    let dir = TempDir::new();
    let data = dir.0.as_path();
    let id = {
        let tasks = service(data);
        let id = full_task(data, &tasks);
        tasks
            .create_task(Some(&json!({ "title": "Other" })), &[], true)
            .unwrap();
        tasks.close();
        id
    };
    // The crash came right after the marker was written.
    fs::write(deleting_marker(data, &id), b"").unwrap();

    let mut index = index_of(data);
    assert_eq!(sweep_orphans(data, &mut index).unwrap(), vec![id.clone()]);

    assert_eq!(holders(data, &[id.as_str(), MARK]), Vec::<String>::new());
    assert_eq!(index.get_task(&id).unwrap(), None);
    let before = snapshot(data);
    assert!(sweep_orphans(data, &mut index).unwrap().is_empty());
    assert_eq!(snapshot(data), before);
    assert_eq!(index.list_tasks(None).unwrap().len(), 1);
}

#[test]
fn the_start_sweep_removes_leftovers_of_a_task_with_no_file_and_no_row() {
    let dir = TempDir::new();
    let data = dir.0.as_path();
    let live = {
        let tasks = service(data);
        let live = tasks
            .create_task(Some(&json!({ "title": "Live" })), &[], true)
            .unwrap();
        save_screen(data, &live.id, "1", b"live screen").unwrap();
        tasks.close();
        live.id
    };
    save_screen(data, "T-0042", "1", MARK.as_bytes()).unwrap();
    fs::write(data.join("agent-launch-T-0042.sh"), MARK).unwrap();

    let mut index = index_of(data);
    assert_eq!(
        sweep_orphans(data, &mut index).unwrap(),
        vec!["T-0042".to_string()]
    );

    assert_eq!(holders(data, &["T-0042", MARK]), Vec::<String>::new());
    assert!(screens_dir(data, &live).join("1.ans").is_file());
    assert!(task_file_path(data, &live).is_file());
}
