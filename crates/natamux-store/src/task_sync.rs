//! The tasks in memory, kept in line with the task files, their event logs and the index
//! (`task-sync.ts`). The owner holds it under one lock; `on_changed` must not take that lock (the
//! daemon queues a `task.changed` message and sends it after). The disk part of a change may run
//! after that lock (`defer_writes`); every read of a task's files or events waits for it first.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, MutexGuard};

use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::Task;

use crate::event_log::{LoggedEvent, event_log_path, read_event_log, write_event_log};
use crate::task_files::{
    FileStat, ReadError, list_task_ids, read_task_file, same_stat, stat_file, task_file_path,
};
use crate::task_index::{TaskFiles, TaskIndex};
use crate::task_writes::{Disk, TaskSlot, Undo, WriteError, WriteJob, Writes};
use crate::watch::TaskWatch;

pub use crate::task_writes::StoreError;

/// How often a read looks at the task files at most, for an edit by hand that the watcher missed:
/// some file systems give no events. A task read stats one file; a list reads the folder.
pub const STAT_CHECK_MS: i64 = 1000;
pub const LIST_CHECK_MS: i64 = 2000;

/// Milliseconds since 1970.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub struct TaskSync {
    disk: Disk,
    /// Every task by id, so a read touches no disk.
    pub tasks: HashMap<String, Task>,
    /// The order of each task's writes, and the stat of its file as this service last wrote or
    /// read it.
    slots: HashMap<String, Arc<TaskSlot>>,
    /// While on, `store` leaves its disk work in `later`.
    defer: bool,
    later: Vec<WriteJob>,
    /// The first error of a write in `later` that ran early, under the lock.
    later_failed: Option<StoreError>,
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
            disk: Disk::new(data_dir.to_path_buf(), index),
            tasks: HashMap::new(),
            slots: HashMap::new(),
            defer: false,
            later: Vec::new(),
            later_failed: None,
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
        &self.disk.data_dir
    }

    /// The index, shared with the writes that run after the store lock.
    pub fn index(&self) -> MutexGuard<'_, TaskIndex> {
        self.disk.index()
    }

    /// Loads every task into memory and brings the index in line with the files. A file with the
    /// time and size the index saw last is not read again. The caller starts the watch.
    pub fn init(&mut self) -> Result<(), StoreError> {
        self.settle_all();
        let known = self.index().task_files().unwrap_or_default();
        let indexed = self.indexed_tasks();
        let now = (self.clock)();
        self.tasks.clear();
        self.slots.clear();
        self.checked_at.clear();
        self.folder_checked_at = now;
        let mut files: Vec<(String, TaskFiles)> = Vec::new();
        // Whether the index differs from the files: a file read again, or an event log that
        // changed.
        let mut changed = false;
        for id in list_task_ids(self.data_dir())? {
            let stat = stat_file(&task_file_path(self.data_dir(), &id))?;
            let known_files = known.get(&id).copied().unwrap_or_default();
            let cached = if same_stat(stat, known_files.task) {
                indexed.get(&id).cloned()
            } else {
                None
            };
            changed |= cached.is_none();
            let task = match cached {
                Some(task) => Some(task),
                // One broken file must not stop the daemon: the task as the index saw it last
                // stays, or none. The file is read again when it changes.
                None => match read_task_file(self.data_dir(), &id) {
                    Ok(task) => task,
                    Err(error) => {
                        log_broken(&id, &error);
                        indexed.get(&id).cloned()
                    }
                },
            };
            let (Some(task), Some(stat)) = (task, stat) else {
                if let Some(stat) = stat {
                    self.slot(&id).set_stat(Some(stat));
                }
                continue;
            };
            let log = self.sync_event_log(&id, known_files.log)?;
            changed |= !same_stat(log, known_files.log);
            self.tasks.insert(id.clone(), task);
            self.slot(&id).set_stat(Some(stat));
            self.checked_at.insert(id.clone(), now);
            files.push((
                id,
                TaskFiles {
                    task: Some(stat),
                    log,
                },
            ));
        }
        // Every task above came from the index unchanged, so the sets are equal when the counts
        // are: a task or file gone from disk, or a new one, shows in the counts.
        changed |= indexed.len() != files.len() || known.len() != files.len();
        if changed {
            let tasks: Vec<&Task> = self.tasks.values().collect();
            self.index()
                .replace_all_tasks(&tasks, &files)
                .map_err(StoreError::Failed)?;
        }
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
        self.index()
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
        let path = event_log_path(self.data_dir(), task_id);
        let stat = stat_file(&path)?;
        if same_stat(stat, known) {
            return Ok(stat);
        }
        let Some(logged) = read_event_log(self.data_dir(), task_id)? else {
            let events: Vec<LoggedEvent> = self
                .index()
                .list_events(task_id)
                .map_err(StoreError::Failed)?
                .into_iter()
                .map(|stored| LoggedEvent {
                    event: stored.event,
                    accepted: stored.accepted,
                })
                .collect();
            write_event_log(self.data_dir(), task_id, &events)?;
            return Ok(stat_file(&path)?);
        };
        let count = self
            .index()
            .count_events(task_id)
            .map_err(StoreError::Failed)?;
        if logged.len() as u64 != count {
            self.index()
                .replace_events(task_id, &logged)
                .map_err(StoreError::Failed)?;
        }
        Ok(stat)
    }

    /// A task file changed on disk. Unless this service wrote it, the task is read again.
    fn reload(&mut self, task_id: &str) -> Result<(), ReadError> {
        // After this service's writes of the task: one between its rename and its stat must not
        // look like an edit by hand.
        let known = self.known_stat(task_id);
        let stat = stat_file(&task_file_path(self.data_dir(), task_id))?;
        if same_stat(stat, known) {
            return Ok(());
        }
        // A file gone from outside keeps the task: task.deleted would end its agents.
        let Some(stat) = stat else {
            return Ok(());
        };
        // Before the read, so a broken file is read again only when it changes.
        self.slot(task_id).set_stat(Some(stat));
        let Some(task) = read_task_file(self.data_dir(), task_id)? else {
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
        let ids = match list_task_ids(self.data_dir()) {
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
    /// transaction. Without `write_file` the file keeps the task as it was. A file or log that
    /// cannot be written leaves the map as it was; a failed index does not.
    pub fn store(
        &mut self,
        task: Task,
        events: &[LoggedEvent],
        write_file: bool,
    ) -> Result<(), StoreError> {
        if write_file {
            self.refuse_unread_edit(&task.id)?;
        }
        let slot = self.slot(&task.id);
        if self.defer {
            if write_file {
                self.written_at
                    .insert(task.id.clone(), task.updated_at.clone());
            }
            let previous = self.tasks.insert(task.id.clone(), task.clone());
            let job = WriteJob::new(slot, task, previous, events.to_vec(), write_file);
            self.later.push(job);
            return Ok(());
        }
        // Waits only for writes other calls left for later; those never wait for this lock.
        let mut turn = slot.wait_turn(slot.ticket());
        let written = self.disk.write(&mut turn, &task, events, write_file);
        drop(turn);
        let (file_written, files_written) = match &written {
            Ok(()) | Err(WriteError::Index(_)) => (true, true),
            Err(WriteError::Log(_)) => (true, false),
            Err(WriteError::File(_)) => (false, false),
        };
        if write_file && file_written {
            self.written_at
                .insert(task.id.clone(), task.updated_at.clone());
        }
        if files_written {
            self.tasks.insert(task.id.clone(), task);
        }
        written.map_err(WriteError::into_error)
    }

    /// The task in memory may be older than an edit by hand. Then that edit is read and the
    /// change is refused. shortcut: an edit between this stat and the write is still lost; that
    /// needs a file lock.
    fn refuse_unread_edit(&mut self, task_id: &str) -> Result<(), StoreError> {
        if !self.started {
            return Ok(());
        }
        let known = self.known_stat(task_id);
        let stat = stat_file(&task_file_path(self.data_dir(), task_id))?;
        if stat.is_none() || same_stat(stat, known) {
            return Ok(());
        }
        self.checked_at.insert(task_id.to_string(), (self.clock)());
        self.try_reload(task_id);
        Err(StoreError::FileChanged(task_id.to_string()))
    }

    /// A deleted task leaves memory too.
    pub fn forget(&mut self, task_id: &str) {
        self.settle(task_id);
        self.tasks.remove(task_id);
        self.slots.remove(task_id);
        self.written_at.remove(task_id);
        self.checked_at.remove(task_id);
    }

    /// From now until `take_writes`, `store` leaves the disk work of a change for after the store
    /// lock: the change is in memory at once, its files and index row once `Writes::run` returns.
    pub fn defer_writes(&mut self) {
        self.defer = true;
    }

    /// The writes `store` left since `defer_writes`, which ends; None when there are none.
    pub fn take_writes(&mut self) -> Option<Writes> {
        self.defer = false;
        if self.later.is_empty() && self.later_failed.is_none() {
            return None;
        }
        Some(Writes {
            disk: self.disk.clone(),
            jobs: std::mem::take(&mut self.later),
            failed: self.later_failed.take(),
        })
    }

    /// A change whose files could not be written leaves memory, as a store under the lock does,
    /// unless a later change or a read of the file replaced it since.
    pub fn undo(&mut self, undo: Undo) {
        if self.tasks.get(&undo.written.id) != Some(&undo.written) {
            return;
        }
        (self.on_changed)(&undo.previous);
        self.tasks.insert(undo.previous.id.clone(), undo.previous);
    }

    /// Runs the writes this call left for later now, under the lock, so a wait for the writes of
    /// a task never waits for this call itself.
    fn run_later(&mut self) {
        for job in std::mem::take(&mut self.later) {
            if let Err((error, undo)) = job.run(&self.disk) {
                if self.later_failed.is_none() {
                    self.later_failed = Some(error);
                }
                if let Some(undo) = undo {
                    self.undo(*undo);
                }
            }
        }
    }

    fn slot(&mut self, task_id: &str) -> Arc<TaskSlot> {
        if let Some(slot) = self.slots.get(task_id) {
            return Arc::clone(slot);
        }
        let slot = Arc::new(TaskSlot::default());
        self.slots.insert(task_id.to_string(), Arc::clone(&slot));
        slot
    }

    /// The stat of a task file as this service last wrote or read it, once every write of the
    /// task decided so far is done.
    fn known_stat(&mut self, task_id: &str) -> Option<FileStat> {
        self.run_later();
        let slot = self.slots.get(task_id)?;
        slot.wait_idle();
        slot.stat()
    }

    /// Waits for every write of the task decided so far, so its files and events are whole.
    pub fn settle(&mut self, task_id: &str) {
        self.known_stat(task_id);
    }

    fn settle_all(&mut self) {
        self.run_later();
        for slot in self.slots.values() {
            slot.wait_idle();
        }
    }

    /// The events of a task, oldest first, with those of every change decided so far.
    pub fn list_events(&mut self, task_id: &str) -> Result<Vec<StoredEvent>, String> {
        self.settle(task_id);
        self.index().list_events(task_id)
    }
}

#[cfg(test)]
#[path = "task_sync_tests.rs"]
mod tests;
