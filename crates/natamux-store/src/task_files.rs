//! The task files: `<data>/tasks/<ID>.md`, the fields in the front matter and the notes as the
//! body (`task-files.ts`).

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use natamux_core::tasks::task::{upgrade_task_fields, validate_task};
use natamux_proto::tasks::task::Task;
use serde_json::{Map, Value};

use crate::front_matter;
use crate::yaml_load::date_of;

/// The folder in a worktree where agents read and write the stage files. Git ignores it.
pub const STAGE_DIR: &str = ".natamux-task";

pub fn tasks_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("tasks")
}

pub fn task_file_path(data_dir: &Path, id: &str) -> PathBuf {
    tasks_dir(data_dir).join(format!("{id}.md"))
}

/// The archived copies of a task's stage files, and its attachments.
pub fn archive_dir(data_dir: &Path, task_id: &str) -> PathBuf {
    tasks_dir(data_dir).join(task_id)
}

/// Enough to tell that a file changed without reading it. `mtime_ms` as Node's `mtimeMs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FileStat {
    pub mtime_ms: f64,
    pub size: u64,
}

pub fn stat_file(path: &Path) -> io::Result<Option<FileStat>> {
    let meta = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let since = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // libuv's seconds and nanoseconds, as Node joins them.
    let mtime_ms = since.as_secs() as f64 * 1e3 + f64::from(since.subsec_nanos()) / 1e6;
    Ok(Some(FileStat {
        mtime_ms,
        size: meta.len(),
    }))
}

pub fn same_stat(a: Option<FileStat>, b: Option<FileStat>) -> bool {
    matches!((a, b), (Some(a), Some(b)) if a == b)
}

/// A task file that could not be read: its YAML, or a field the schema refuses.
#[derive(Debug)]
pub enum ReadError {
    Io(io::Error),
    Broken(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io(error) => write!(f, "{error}"),
            ReadError::Broken(text) => f.write_str(text),
        }
    }
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> ReadError {
        ReadError::Io(error)
    }
}

/// YAML turns an unquoted timestamp into a date; the task schema wants a string. Only the fields
/// at the top level are turned, as `dateToIsoString` does.
fn dates_to_text(data: Map<String, Value>) -> Map<String, Value> {
    data.into_iter()
        .map(|(key, value)| match date_of(&value) {
            Some(iso) => (key, Value::String(iso.to_string())),
            None => (key, value),
        })
        .collect()
}

pub fn parse_task_file(text: &str) -> Result<Task, ReadError> {
    let (data, content) = front_matter::parse(text).map_err(ReadError::Broken)?;
    let Value::Object(data) = data else {
        return Err(ReadError::Broken(
            "The front matter is not a mapping".into(),
        ));
    };
    let mut fields = dates_to_text(data);
    fields.insert(
        "description".into(),
        Value::String(natamux_core::js::js_trim(&content).to_string()),
    );
    let fields = upgrade_task_fields(fields);
    validate_task(&Value::Object(fields)).map_err(|error| ReadError::Broken(error.to_json_text()))
}

/// The fields in the front matter, the notes as the body.
pub fn serialize_task(task: &Task) -> String {
    let mut value = serde_json::to_value(task).unwrap_or(Value::Null);
    let description = value
        .as_object_mut()
        .and_then(|map| map.shift_remove("description"))
        .and_then(|text| text.as_str().map(str::to_string))
        .unwrap_or_default();
    front_matter::stringify(&format!("{description}\n"), &value)
}

/// Written through a `.tmp` file and a rename. Returns the stat of the new file, so a change seen
/// later can be told from this write.
pub fn write_task_file(data_dir: &Path, task: &Task) -> io::Result<FileStat> {
    fs::create_dir_all(tasks_dir(data_dir))?;
    let path = task_file_path(data_dir, &task.id);
    write_atomic(&path, serialize_task(task).as_bytes())?;
    stat_file(&path)?.ok_or_else(|| io::Error::from(ErrorKind::NotFound))
}

/// `writeFileSync(path.tmp)` then `renameSync`.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

/// None for a task that has no file.
pub fn read_task_file(data_dir: &Path, id: &str) -> Result<Option<Task>, ReadError> {
    match fs::read(task_file_path(data_dir, id)) {
        Ok(bytes) => parse_task_file(&String::from_utf8_lossy(&bytes)).map(Some),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// `/^([A-Z]+-\d+)\.md$/`.
pub fn task_id_of_file(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".md")?;
    natamux_core::formats::is_task_id_shape(id).then_some(id)
}

fn id_number(id: &str) -> f64 {
    id.rsplit('-')
        .next()
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(f64::NAN)
}

/// Every task id with a file, by the number at the end, then as text.
pub fn list_task_ids(data_dir: &Path) -> io::Result<Vec<String>> {
    let entries = match fs::read_dir(tasks_dir(data_dir)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            task_id_of_file(&entry.file_name().to_string_lossy()).map(str::to_string)
        })
        .collect();
    // Ids are upper case letters, '-' and digits: byte order is the order of localeCompare.
    ids.sort_by(|a, b| {
        id_number(a)
            .partial_cmp(&id_number(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });
    Ok(ids)
}

pub fn list_task_files(data_dir: &Path) -> Result<Vec<Task>, ReadError> {
    let mut tasks = Vec::new();
    for id in list_task_ids(data_dir)? {
        if let Some(task) = read_task_file(data_dir, &id)? {
            tasks.push(task);
        }
    }
    Ok(tasks)
}

#[cfg(test)]
#[path = "task_files_tests.rs"]
mod tests;
