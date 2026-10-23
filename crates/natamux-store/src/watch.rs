//! Tells which task files changed (`watchTaskFiles`). Events are gathered for a moment, so an
//! editor that saves in several steps causes one call. Best effort: a folder that cannot be
//! watched is not, and the daemon reads it again at the next start.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::task_files::{task_id_of_file, tasks_dir};

pub const WATCH_DELAY: Duration = Duration::from_millis(50);

/// Stops the watch when dropped.
pub struct TaskWatch {
    _watcher: Option<RecommendedWatcher>,
}

pub fn watch_task_files(
    data_dir: &Path,
    delay: Duration,
    on_change: impl Fn(BTreeSet<String>) + Send + 'static,
) -> TaskWatch {
    let folder = tasks_dir(data_dir);
    let (sender, names) = mpsc::channel::<String>();
    let watcher = std::fs::create_dir_all(&folder).ok().and_then(|()| {
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                for path in event.paths {
                    if let Some(name) = path.file_name() {
                        let _ = sender.send(name.to_string_lossy().into_owned());
                    }
                }
            })
            .ok()?;
        watcher.watch(&folder, RecursiveMode::NonRecursive).ok()?;
        Some(watcher)
    });
    // Ends when the watcher, and with it the sender, is dropped.
    thread::spawn(move || {
        while let Ok(first) = names.recv() {
            // The wait starts at the first change of a task file.
            if task_id_of_file(&first).is_none() {
                continue;
            }
            let mut ids = BTreeSet::new();
            let mut take = |name: String| {
                if let Some(id) = task_id_of_file(&name) {
                    ids.insert(id.to_string());
                }
            };
            take(first);
            let until = Instant::now() + delay;
            while let Some(left) = until.checked_duration_since(Instant::now()) {
                match names.recv_timeout(left) {
                    Ok(name) => take(name),
                    Err(_) => break,
                }
            }
            if !ids.is_empty() {
                on_change(ids);
            }
        }
    });
    TaskWatch { _watcher: watcher }
}
