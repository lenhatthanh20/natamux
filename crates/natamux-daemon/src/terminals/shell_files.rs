//! The files of a task's shells beside its task file (`open-shells-file.ts`, `ended-shells.ts`):
//! the shells that were open, so they come back after a restart, and the last screens of the
//! shells of a done or cancelled task.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use natamux_proto::terminals::ended::{ENDED_SHELL_MAX_CHARS, EndedShell};
use natamux_proto::terminals::query::is_terminal_name;
use serde::{Deserialize, Serialize};

/// A shell that was open: the agent that ran in it and the last session that agent reported.
/// None: a plain shell, or no session yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenShell {
    pub name: String,
    pub agent: Option<String>,
    pub session: Option<String>,
}

pub fn tasks_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tasks")
}

pub fn open_shells_path(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{task_id}.open-shells.json"))
}

/// Beside the task file, not in the stage-files folder, so the Docs tab does not list it.
pub fn ended_shells_path(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{task_id}.shells.json"))
}

/// Written whole to a temporary file first, so a crash never leaves half a file.
fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)?;
    }
    let text = serde_json::to_string(value).map_err(io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

pub fn save_open_shells(data_dir: &Path, task_id: &str, shells: &[OpenShell]) -> io::Result<()> {
    write_json(&open_shells_path(data_dir, task_id), &shells)
}

/// A missing or broken file reads as no shells: nothing comes back, the daemon still starts.
pub fn read_open_shells(data_dir: &Path, task_id: &str) -> Vec<OpenShell> {
    let Ok(text) = fs::read_to_string(open_shells_path(data_dir, task_id)) else {
        return Vec::new();
    };
    let shells: Vec<OpenShell> = serde_json::from_str(&text).unwrap_or_default();
    let valid = shells.iter().all(|shell| {
        is_terminal_name(&shell.name)
            && shell.agent.as_ref().is_none_or(|agent| !agent.is_empty())
            && shell
                .session
                .as_ref()
                .is_none_or(|session| !session.is_empty())
    });
    if valid { shells } else { Vec::new() }
}

pub fn delete_open_shells(data_dir: &Path, task_id: &str) -> io::Result<()> {
    remove(&open_shells_path(data_dir, task_id))
}

/// The tasks that have an open-shells file, for the start of the daemon.
pub fn tasks_with_open_shells(data_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(tasks_dir(data_dir)) else {
        return Vec::new();
    };
    let mut tasks: Vec<String> = entries
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".open-shells.json").map(String::from)
        })
        .collect();
    tasks.sort();
    tasks
}

/// The end of a long screen is what the user wants to see.
pub fn save_ended_shells(data_dir: &Path, task_id: &str, shells: &[EndedShell]) -> io::Result<()> {
    let kept: Vec<EndedShell> = shells
        .iter()
        .map(|shell| EndedShell {
            text: last_units(&shell.text, ENDED_SHELL_MAX_CHARS),
            ..shell.clone()
        })
        .collect();
    write_json(&ended_shells_path(data_dir, task_id), &kept)
}

/// The end of `text`, at most `max` UTF-16 units (JavaScript's `slice(-max)`), never half a
/// character.
fn last_units(text: &str, max: usize) -> String {
    let mut units = 0;
    let mut start = text.len();
    for (index, char) in text.char_indices().rev() {
        units += char.len_utf16();
        if units > max {
            break;
        }
        start = index;
    }
    text[start..].to_string()
}

/// A missing or broken file reads as no shells: the screens are a convenience, never a reason to
/// fail a request.
pub fn read_ended_shells(data_dir: &Path, task_id: &str) -> Vec<EndedShell> {
    let Ok(text) = fs::read_to_string(ended_shells_path(data_dir, task_id)) else {
        return Vec::new();
    };
    let shells: Vec<EndedShell> = serde_json::from_str(&text).unwrap_or_default();
    let valid = shells.iter().all(|shell| {
        (1..=16).contains(&shell.name.encode_utf16().count())
            && shell.agent.as_ref().is_none_or(|agent| !agent.is_empty())
            && shell.stage.as_ref().is_none_or(|stage| !stage.is_empty())
            && shell
                .session
                .as_ref()
                .is_none_or(|session| !session.is_empty())
            && !shell.ended_at.is_empty()
            && shell.text.encode_utf16().count() <= ENDED_SHELL_MAX_CHARS
    });
    if valid { shells } else { Vec::new() }
}

/// A reopened agent reported a newer session, so the next reopen goes on with it. Only a file
/// that exists is written: a deleted task must not get one back.
pub fn note_ended_session(
    data_dir: &Path,
    task_id: &str,
    terminal: &str,
    agent: Option<&str>,
    session: Option<&str>,
) -> io::Result<()> {
    let (Some(agent), Some(session)) = (agent, session) else {
        return Ok(());
    };
    let mut shells = read_ended_shells(data_dir, task_id);
    let Some(shell) = shells.iter_mut().find(|shell| shell.name == terminal) else {
        return Ok(());
    };
    if shell.agent.as_deref() != Some(agent) || shell.session.as_deref() == Some(session) {
        return Ok(());
    }
    shell.session = Some(session.to_string());
    save_ended_shells(data_dir, task_id, &shells)
}

pub fn delete_ended_shells(data_dir: &Path, task_id: &str) -> io::Result<()> {
    remove(&ended_shells_path(data_dir, task_id))
}
