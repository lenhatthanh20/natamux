//! Files the daemon writes in the data folder are only the user's: files 0600, folders 0700.
//! Files an older version wrote wider are tightened at start.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::start_privacy::tighten_modes;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::terminals::screen_files::save_screen;
use natamux_daemon::terminals::shell_files::{OpenShell, save_open_shells};
use natamux_store::attachments::AttachmentFile;
use natamux_store::task_index::TaskIndex;
use serde_json::json;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "natamux-modes-{}-{}",
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

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
}

fn set_mode(path: &Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

/// Every path under `folder` with a mode other than 0600 for a file or 0700 for a folder.
fn wide_paths(folder: &Path) -> Vec<String> {
    let mut wide = Vec::new();
    for entry in fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        let want = if path.is_dir() { 0o700 } else { 0o600 };
        if mode(&path) != want {
            wide.push(format!("{} {:o}", path.display(), mode(&path)));
        }
        if path.is_dir() {
            wide.extend(wide_paths(&path));
        }
    }
    wide
}

#[test]
fn new_files_under_the_data_folder_are_0600_and_folders_0700() {
    let dir = TempDir::new();
    let data = dir.0.as_path();
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: data.to_path_buf(),
        index: TaskIndex::open(&data.join("index.sqlite")).unwrap(),
        bus: EventBus::new(),
        now: None,
        settings_of: None,
        clock: None,
    });
    let attachment = AttachmentFile {
        name: "notes.txt".into(),
        bytes: b"attached".to_vec(),
    };
    let task = tasks
        .create_task(Some(&json!({ "title": "Private" })), &[attachment], true)
        .unwrap();
    tasks
        .update_task(&task.id, Some(&json!({ "title": "Renamed" })))
        .unwrap();
    save_screen(data, &task.id, "1", b"screen").unwrap();
    let shell = OpenShell {
        name: "1".into(),
        agent: None,
        session: None,
        paused: false,
    };
    save_open_shells(data, &task.id, &[shell]).unwrap();

    assert!(data.join("tasks").join(format!("{}.md", task.id)).is_file());
    assert!(data.join("index.sqlite-wal").is_file());
    assert_eq!(wide_paths(data), Vec::<String>::new());
}

#[test]
fn old_wider_files_are_tightened_at_start_and_worktrees_are_left_alone() {
    let dir = TempDir::new();
    let data = dir.0.as_path();
    let tasks = data.join("tasks");
    let worktree = data.join("worktrees").join("T-0001");
    fs::create_dir_all(tasks.join("T-0001")).unwrap();
    fs::create_dir_all(&worktree).unwrap();
    fs::write(data.join("repo-trust.json"), b"{}").unwrap();
    fs::write(tasks.join("T-0001.md"), b"x").unwrap();
    for (path, wide) in [
        (data.to_path_buf(), 0o755),
        (tasks.clone(), 0o755),
        (tasks.join("T-0001"), 0o755),
        (data.join("repo-trust.json"), 0o644),
        (tasks.join("T-0001.md"), 0o666),
        (worktree.clone(), 0o755),
    ] {
        set_mode(&path, wide);
    }

    tighten_modes(data, &data.join("worktrees"));

    assert_eq!(mode(data), 0o700);
    assert_eq!(mode(&tasks), 0o700);
    assert_eq!(mode(&tasks.join("T-0001")), 0o700);
    assert_eq!(mode(&data.join("repo-trust.json")), 0o600);
    assert_eq!(mode(&tasks.join("T-0001.md")), 0o600);
    assert_eq!(mode(&worktree), 0o755);
}
