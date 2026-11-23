//! Removes every file a task left in the data folder: its task file, events, usage, shells,
//! screens, archive and attachments, and its launch and stage-command scripts. A delete writes a
//! marker first, so a crash in the middle is finished at the next start.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use natamux_core::formats::is_task_id_shape;
use natamux_core::workspaces::private_dir::{private_dir, write_private};
use natamux_store::task_files::{task_file_path, tasks_dir};
use natamux_store::task_index::TaskIndex;

const MARKER_SUFFIX: &str = ".deleting";
const SCRIPT_PREFIXES: [&str; 2] = ["agent-launch-", "stage-command-"];

/// `tasks/<id>.deleting`: a delete of the task started and has not finished.
pub fn deleting_marker(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{task_id}{MARKER_SUFFIX}"))
}

pub fn mark_deleting(data_dir: &Path, task_id: &str) -> io::Result<()> {
    checked(task_id)?;
    private_dir(&tasks_dir(data_dir))?;
    write_private(&deleting_marker(data_dir, task_id), b"")
}

pub fn unmark_deleting(data_dir: &Path, task_id: &str) -> io::Result<()> {
    remove_path(&deleting_marker(data_dir, task_id))
}

/// Every file and folder of the task, found by name, so a temporary file of a crashed write is
/// found too. The marker is not in the list: it goes last.
pub fn task_paths(data_dir: &Path, task_id: &str) -> io::Result<Vec<PathBuf>> {
    checked(task_id)?;
    let tasks = tasks_dir(data_dir);
    let mut paths: Vec<PathBuf> = names_in(&tasks)?
        .into_iter()
        .filter(|name| task_of_task_entry(name) == Some(task_id))
        .filter(|name| !name.ends_with(MARKER_SUFFIX))
        .map(|name| tasks.join(name))
        .collect();
    paths.extend(
        names_in(data_dir)?
            .into_iter()
            .filter(|name| task_of_script(name) == Some(task_id))
            .map(|name| data_dir.join(name)),
    );
    paths.sort();
    Ok(paths)
}

/// Removes the paths of `task_paths`. A path that is gone already is fine, so a second run
/// changes nothing.
pub fn purge_task_files(data_dir: &Path, task_id: &str) -> io::Result<()> {
    for path in task_paths(data_dir, task_id)? {
        remove_path(&path)?;
    }
    Ok(())
}

/// At start, before the tasks load: finishes each delete that has a marker, then removes the
/// files of tasks that have neither a task file nor an index row. Returns the ids it cleaned.
pub fn sweep_orphans(data_dir: &Path, index: &mut TaskIndex) -> io::Result<Vec<String>> {
    let mut cleaned = Vec::new();
    let names = names_in(&tasks_dir(data_dir))?;
    for task_id in names
        .iter()
        .filter_map(|name| name.strip_suffix(MARKER_SUFFIX))
    {
        if !is_task_id_shape(task_id) {
            continue;
        }
        purge_task_files(data_dir, task_id)?;
        index.delete_task(task_id).map_err(io::Error::other)?;
        unmark_deleting(data_dir, task_id)?;
        cleaned.push(task_id.to_string());
    }
    let mut ids: Vec<String> = names
        .iter()
        .filter_map(|name| task_of_task_entry(name))
        .chain(
            names_in(data_dir)?
                .iter()
                .filter_map(|name| task_of_script(name)),
        )
        .map(str::to_string)
        .collect();
    ids.sort();
    ids.dedup();
    for task_id in ids {
        if cleaned.contains(&task_id) || task_file_path(data_dir, &task_id).exists() {
            continue;
        }
        // A row that cannot be read is kept as a task: only a sure "no row" removes files.
        if !matches!(index.get_task(&task_id), Ok(None)) {
            continue;
        }
        purge_task_files(data_dir, &task_id)?;
        cleaned.push(task_id);
    }
    Ok(cleaned)
}

fn checked(task_id: &str) -> io::Result<()> {
    if is_task_id_shape(task_id) {
        Ok(())
    } else {
        Err(io::Error::other(format!("Not a task id: {task_id:?}")))
    }
}

fn names_in(folder: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut names = Vec::new();
    for entry in entries {
        if let Ok(name) = entry?.file_name().into_string() {
            names.push(name);
        }
    }
    Ok(names)
}

/// The leading `T-0001` of a name, so `T-1` is never read as part of `T-10`.
fn id_prefix(name: &str) -> Option<(&str, &str)> {
    let letters = name.bytes().take_while(u8::is_ascii_uppercase).count();
    let after = name.get(letters..)?.strip_prefix('-')?;
    let digits = after.bytes().take_while(u8::is_ascii_digit).count();
    if letters == 0 || digits == 0 {
        return None;
    }
    let end = letters + 1 + digits;
    Some((name.get(..end)?, name.get(end..)?))
}

/// `T-0001`, `T-0001.md`, `T-0001.screens`, `T-0001.md.tmp`, ...: the task of an entry of the
/// tasks folder.
fn task_of_task_entry(name: &str) -> Option<&str> {
    let (id, rest) = id_prefix(name)?;
    (rest.is_empty() || rest.starts_with('.')).then_some(id)
}

/// `agent-launch-T-0001.sh`, `agent-launch-T-0001-2.sh`, `stage-command-T-0001.sh`.
fn task_of_script(name: &str) -> Option<&str> {
    let rest = SCRIPT_PREFIXES
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))?;
    let (id, after) = id_prefix(rest)?;
    let tail = after.strip_prefix('-').map_or(after, |terminal| {
        terminal.trim_start_matches(|char: char| char.is_ascii_alphanumeric())
    });
    (tail == ".sh").then_some(id)
}

fn remove_path(path: &Path) -> io::Result<()> {
    let result = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) => Err(error),
    };
    match result {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
#[path = "purge_tests.rs"]
mod tests;
