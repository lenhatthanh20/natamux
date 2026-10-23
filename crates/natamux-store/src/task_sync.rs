//! The tasks in memory, kept in line with the task files, their event logs and the index
//! (`task-sync.ts`). The owner holds it under one lock; `on_changed` must not take that lock (the
//! daemon queues a `task.changed` message and sends it after).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use natamux_proto::tasks::task::Task;

use crate::event_log::{
    LoggedEvent, append_event_line, event_log_path, read_event_log, write_event_log,
};
use crate::task_files::{
    FileStat, ReadError, list_task_ids, read_task_file, same_stat, stat_file, task_file_path,
    write_task_file,
};
use crate::task_index::{TaskChange, TaskFiles, TaskIndex};
use crate::watch::TaskWatch;

/// How often a read looks at the task files at most, for an edit by hand that the watcher missed:
/// some file systems give no events. A task read stats one file; a list reads the folder.
pub const STAT_CHECK_MS: i64 = 1000;
pub const LIST_CHECK_MS: i64 = 2000;

/// Milliseconds since 1970.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// A store refused or failed: the task file changed by hand, or the disk or index failed.
#[derive(Debug)]
pub enum StoreError {
    FileChanged(String),
    Failed(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::FileChanged(task_id) => write!(
                f,
                "The file of task {task_id} was changed outside Natamux. It is read again; try again"
            ),
            StoreError::Failed(text) => f.write_str(text),
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> StoreError {
        StoreError::Failed(error.to_string())
    }
}

pub struct TaskSync {
    data_dir: PathBuf,
    pub index: TaskIndex,
    /// Every task by id, so a read touches no disk.
    pub tasks: HashMap<String, Task>,
    /// The stat of each task file as this service last wrote or read it.
    file_stats: HashMap<String, FileStat>,
    /// The `updatedAt` of each task when its file was last written.
    pub written_at: HashMap<String, String>,
    clock: Clock,
    checked_at: HashMap<String, i64>,
    folder_checked_at: i64,
    pub started: bool,
    watch: Option<TaskWatch>,
    on_changed: Box<dyn Fn(&Task) + Send>,
}

fn log_broken(task_id: &str, error: &dyn std::fmt::Display) {
    eprintln!("Could not read the task file of {task_id}: {error}");
}

impl TaskSync {
    pub fn new(
        data_dir: &Path,
        index: TaskIndex,
        clock: Clock,
        on_changed: Box<dyn Fn(&Task) + Send>,
    ) -> TaskSync {
        TaskSync {
            data_dir: data_dir.to_path_buf(),
            index,
            tasks: HashMap::new(),
            file_stats: HashMap::new(),
            written_at: HashMap::new(),
            clock,
            checked_at: HashMap::new(),
            folder_checked_at: 0,
            started: false,
            watch: None,
            on_changed,
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Loads every task into memory and brings the index in line with the files. A file with the
    /// time and size the index saw last is not read again. The caller starts the watch.
    pub fn init(&mut self) -> Result<(), StoreError> {
        let known = self.index.task_files().unwrap_or_default();
        let indexed = self.indexed_tasks();
        let now = (self.clock)();
        self.tasks.clear();
        self.file_stats.clear();
        self.checked_at.clear();
        self.folder_checked_at = now;
        let mut files: Vec<(String, TaskFiles)> = Vec::new();
        for id in list_task_ids(&self.data_dir)? {
            let stat = stat_file(&task_file_path(&self.data_dir, &id))?;
            let known_files = known.get(&id).copied().unwrap_or_default();
            let cached = if same_stat(stat, known_files.task) {
                indexed.get(&id).cloned()
            } else {
                None
            };
            let task = match cached {
                Some(task) => Some(task),
                // One broken file must not stop the daemon: the task as the index saw it last
                // stays, or none. The file is read again when it changes.
                None => match read_task_file(&self.data_dir, &id) {
                    Ok(task) => task,
                    Err(error) => {
                        log_broken(&id, &error);
                        indexed.get(&id).cloned()
                    }
                },
            };
            let (Some(task), Some(stat)) = (task, stat) else {
                if let Some(stat) = stat {
                    self.file_stats.insert(id, stat);
                }
                continue;
            };
            let log = self.sync_event_log(&id, known_files.log)?;
            self.tasks.insert(id.clone(), task);
            self.file_stats.insert(id.clone(), stat);
            self.checked_at.insert(id.clone(), now);
            files.push((
                id,
                TaskFiles {
                    task: Some(stat),
                    log,
                },
            ));
        }
        let tasks: Vec<&Task> = self.tasks.values().collect();
        self.index
            .replace_all_tasks(&tasks, &files)
            .map_err(StoreError::Failed)?;
        self.started = true;
        Ok(())
    }

    /// Keeps the watch of the task files until `close`.
    pub fn set_watch(&mut self, watch: TaskWatch) {
        self.watch = Some(watch);
    }

    /// Stops watching the task files.
    pub fn close(&mut self) {
        self.watch = None;
    }

    /// The tasks as the index holds them. A row of an older version may not parse; then every
    /// file is read.
    fn indexed_tasks(&self) -> HashMap<String, Task> {
        self.index
            .list_tasks(None)
            .map(|tasks| {
                tasks
                    .into_iter()
                    .map(|task| (task.id.clone(), task))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Brings the events of a task in the index in line with its event log, when the log changed
    /// since the index saw it. A task with no event log yet gets one from the index.
    fn sync_event_log(
        &mut self,
        task_id: &str,
        known: Option<FileStat>,
    ) -> Result<Option<FileStat>, StoreError> {
        let path = event_log_path(&self.data_dir, task_id);
        let stat = stat_file(&path)?;
        if same_stat(stat, known) {
            return Ok(stat);
        }
        let Some(logged) = read_event_log(&self.data_dir, task_id)? else {
            let events: Vec<LoggedEvent> = self
                .index
                .list_events(task_id)
                .map_err(StoreError::Failed)?
                .into_iter()
                .map(|stored| LoggedEvent {
                    event: stored.event,
                    accepted: stored.accepted,
                })
                .collect();
            write_event_log(&self.data_dir, task_id, &events)?;
            return Ok(stat_file(&path)?);
        };
        let count = self
            .index
            .count_events(task_id)
            .map_err(StoreError::Failed)?;
        if logged.len() as u64 != count {
            self.index
                .replace_events(task_id, &logged)
                .map_err(StoreError::Failed)?;
        }
        Ok(stat)
    }

    /// A task file changed on disk. Unless this service wrote it, the task is read again.
    fn reload(&mut self, task_id: &str) -> Result<(), ReadError> {
        let stat = stat_file(&task_file_path(&self.data_dir, task_id))?;
        if same_stat(stat, self.file_stats.get(task_id).copied()) {
            return Ok(());
        }
        // A file gone from outside keeps the task: task.deleted would end its agents.
        let Some(stat) = stat else {
            return Ok(());
        };
        // Before the read, so a broken file is read again only when it changes.
        self.file_stats.insert(task_id.to_string(), stat);
        let Some(task) = read_task_file(&self.data_dir, task_id)? else {
            return Ok(());
        };
        self.tasks.insert(task_id.to_string(), task.clone());
        // Not task.created, even for a new file: that would start the context stage.
        (self.on_changed)(&task);
        Ok(())
    }

    pub fn try_reload(&mut self, task_id: &str) {
        if let Err(error) = self.reload(task_id) {
            // A half-written or broken file: the last good task stays until the file is fixed.
            log_broken(task_id, &error);
        }
    }

    /// The watcher may miss an edit by hand, so a read stats the task file too, once a second.
    pub fn check_file(&mut self, task_id: &str) {
        if !self.started || !self.tasks.contains_key(task_id) {
            return;
        }
        let now = (self.clock)();
        if now - self.checked_at.get(task_id).copied().unwrap_or(0) < STAT_CHECK_MS {
            return;
        }
        self.checked_at.insert(task_id.to_string(), now);
        self.try_reload(task_id);
    }

    /// The same for every task file, and files added by hand, every two seconds at most.
    pub fn check_folder(&mut self) {
        let now = (self.clock)();
        if !self.started || now - self.folder_checked_at < LIST_CHECK_MS {
            return;
        }
        self.folder_checked_at = now;
        let ids = match list_task_ids(&self.data_dir) {
            Ok(ids) => ids,
            Err(error) => {
                eprintln!("Could not read the task folder: {error}");
                return;
            }
        };
        for id in ids {
            self.checked_at.insert(id.clone(), now);
            self.try_reload(&id);
        }
    }

    /// Stores a change of a task: its file, its event log, the map, then the index in one
    /// transaction. Without `write_file` the file keeps the task as it was.
    pub fn store(
        &mut self,
        task: Task,
        events: &[LoggedEvent],
        write_file: bool,
    ) -> Result<(), StoreError> {
        let mut files = TaskFiles::default();
        if write_file {
            self.refuse_unread_edit(&task.id)?;
            let stat = write_task_file(&self.data_dir, &task)?;
            files.task = Some(stat);
            self.file_stats.insert(task.id.clone(), stat);
            self.written_at
                .insert(task.id.clone(), task.updated_at.clone());
        }
        for logged in events {
            files.log = Some(append_event_line(
                &self.data_dir,
                &logged.event,
                logged.accepted,
            )?);
        }
        self.tasks.insert(task.id.clone(), task.clone());
        let change = TaskChange {
            task: Some(&task),
            events,
            files,
        };
        self.index
            .record(&task.id, &change)
            .map_err(StoreError::Failed)
    }

    /// The task in memory may be older than an edit by hand. Then that edit is read and the
    /// change is refused. shortcut: an edit between this stat and the write is still lost; that
    /// needs a file lock.
    fn refuse_unread_edit(&mut self, task_id: &str) -> Result<(), StoreError> {
        if !self.started {
            return Ok(());
        }
        let stat = stat_file(&task_file_path(&self.data_dir, task_id))?;
        if stat.is_none() || same_stat(stat, self.file_stats.get(task_id).copied()) {
            return Ok(());
        }
        self.checked_at.insert(task_id.to_string(), (self.clock)());
        self.try_reload(task_id);
        Err(StoreError::FileChanged(task_id.to_string()))
    }

    /// A deleted task leaves memory too.
    pub fn forget(&mut self, task_id: &str) {
        self.tasks.remove(task_id);
        self.file_stats.remove(task_id);
        self.written_at.remove(task_id);
        self.checked_at.remove(task_id);
    }
}
