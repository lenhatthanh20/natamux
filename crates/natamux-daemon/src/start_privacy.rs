//! Privacy work at daemon start, before the tasks load: finish deletes a crash cut short, and
//! tighten files an older version wrote wider than only the user.

use std::fs;
use std::path::Path;

use natamux_core::workspaces::private_dir::{private_dir, private_file};
use natamux_store::task_index::TaskIndex;

use crate::tasks::purge::sweep_orphans;

/// A failed sweep is logged and the start goes on: the task files are still the source of truth.
pub fn prepare(data_dir: &Path, worktrees_dir: &Path, index: &mut TaskIndex) {
    tighten_modes(data_dir, worktrees_dir);
    match sweep_orphans(data_dir, index) {
        Ok(cleaned) if !cleaned.is_empty() => {
            eprintln!("Removed the files of deleted tasks: {}", cleaned.join(", "));
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
