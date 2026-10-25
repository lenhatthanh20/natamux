//! One JSON line per event of a task, next to its task file (`event-log.ts`). The index keeps the
//! events too, for fast reads; this file is what survives when the index is lost.

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use natamux_core::events::schema::validate_task_event;
use natamux_core::json::{parse_js, to_js_string};
use natamux_proto::events::task_event::TaskEvent;
use serde_json::Value;

use crate::task_files::{FileStat, stat_file, tasks_dir};

/// An event and whether it was accepted.
#[derive(Clone, Debug, PartialEq)]
pub struct LoggedEvent {
    pub event: TaskEvent,
    pub accepted: bool,
}

pub fn event_log_path(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{task_id}.events.jsonl"))
}

/// `JSON.stringify({ ...event, accepted })`.
pub fn event_line(event: &TaskEvent, accepted: bool) -> String {
    let mut value = serde_json::to_value(event).unwrap_or(Value::Null);
    if let Some(map) = value.as_object_mut() {
        map.insert("accepted".into(), Value::Bool(accepted));
    }
    to_js_string(&value)
}

/// Returns the stat of the log after the line, so the next start knows whether the log changed.
pub fn append_event_line(
    data_dir: &Path,
    event: &TaskEvent,
    accepted: bool,
) -> io::Result<FileStat> {
    let path = event_log_path(data_dir, &event.task_id);
    let line = format!("{}\n", event_line(event, accepted));
    let append = |path: &Path| {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(line.as_bytes())
    };
    match append(&path) {
        // The folder is made only when it is missing, not once per event.
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(tasks_dir(data_dir))?;
            append(&path)?;
        }
        other => other?,
    }
    stat_file(&path)?.ok_or_else(|| io::Error::from(ErrorKind::NotFound))
}

pub fn write_event_log(data_dir: &Path, task_id: &str, events: &[LoggedEvent]) -> io::Result<()> {
    fs::create_dir_all(tasks_dir(data_dir))?;
    let text: Vec<String> = events
        .iter()
        .map(|logged| event_line(&logged.event, logged.accepted))
        .collect();
    let body = if text.is_empty() {
        String::new()
    } else {
        format!("{}\n", text.join("\n"))
    };
    fs::write(event_log_path(data_dir, task_id), body)
}

/// The events of a task, oldest first; None when it has no log. A line that cannot be read (a crash
/// in the middle of a write) is skipped.
pub fn read_event_log(data_dir: &Path, task_id: &str) -> io::Result<Option<Vec<LoggedEvent>>> {
    let text = match fs::read(event_log_path(data_dir, task_id)) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let events = text
        .split('\n')
        .filter(|line| !natamux_core::js::js_trim(line).is_empty())
        .filter_map(|line| {
            let raw = parse_js(line).ok()?;
            let event = validate_task_event(&raw).ok()?;
            let accepted = raw.get("accepted") != Some(&Value::Bool(false));
            Some(LoggedEvent { event, accepted })
        })
        .collect();
    Ok(Some(events))
}

#[cfg(test)]
#[path = "event_log_tests.rs"]
mod tests;
