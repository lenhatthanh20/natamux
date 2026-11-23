//! Privacy work at daemon start. Before the tasks load: finish deletes a crash cut short, and
//! tighten files an older version wrote wider than only the user. After the start: cut the audit
//! log to its cap and start retention.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use natamux_core::privacy::log_trim::trim_file;
use natamux_core::security::data_in;
use natamux_core::workspaces::private_dir::{private_dir, private_file};
use natamux_store::task_index::TaskIndex;

use crate::http::server::AppState;
use crate::privacy::{audit, retention, support_bundle};
use crate::tasks::purge::sweep_orphans;

/// Once the daemon serves: the audit log under its cap, then retention (now and every 24 h).
pub fn after_start(state: &Arc<AppState>) {
    support_bundle::mark_start();
    let cap = data_in(&state.data_dir).log_max_bytes;
    if let Err(error) = trim_file(&audit::audit_path(&state.data_dir), cap) {
        eprintln!("The audit log could not be cut to its size: {error}");
    }
    retention::start(state);
}

/// A failed sweep is logged and the start goes on: the task files are still the source of truth.
pub fn prepare(data_dir: &Path, worktrees_dir: &Path, index: &mut TaskIndex) {
    tighten_modes(data_dir, worktrees_dir);
    match sweep_orphans(data_dir, index) {
        Ok(cleaned) if !cleaned.is_empty() => {
            eprintln!("Removed the files of deleted tasks: {}", cleaned.join(", "));
            for id in &cleaned {
                audit::write(data_dir, "task.purge", id, "ok");
            }
        }
        Ok(_) => {}
        Err(error) => eprintln!("Could not finish the delete of old tasks: {error}"),
    }
}

/// Files 0600 and folders 0700 in the top two levels of the data folder. The worktrees are the
/// user's code, not Natamux data, so they keep their modes. Links are never followed, and a
/// chmod that fails is ignored, as for the token file.
pub fn tighten_modes(data_dir: &Path, worktrees_dir: &Path) {
    let _ = private_dir(data_dir);
    for entry in children(data_dir) {
        if entry == worktrees_dir {
            continue;
        }
        if tighten(&entry) {
            for inner in children(&entry) {
                tighten(&inner);
            }
        }
    }
}

// True for a folder, so its children are looked at too.
fn tighten(path: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.is_dir() {
        let _ = private_dir(path);
        return true;
    }
    if meta.is_file() {
        let _ = private_file(path);
    }
    false
}

fn children(folder: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(folder)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default()
}
