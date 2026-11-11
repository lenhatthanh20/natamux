//! The disk part of a task change: its file, its event log lines and its index row. The store
//! decides a change under its lock and may run this part after it lets the lock go, so the writes
//! of different tasks run at once. Each task has a slot: the writes of one task run one at a
//! time, in the order the store decided them, so its file, log and index end as its last change.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use natamux_proto::tasks::task::Task;

use crate::event_log::{LoggedEvent, append_event_line};
use crate::task_files::{FileStat, write_task_file};
use crate::task_index::{TaskChange, TaskFiles, TaskIndex};

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

/// How far a failed write got.
#[derive(Debug)]
pub enum WriteError {
    /// Nothing was written.
    File(StoreError),
    /// The task file was written when asked; an event line was not.
    Log(StoreError),
    /// The files were written; the index was not.
    Index(StoreError),
}

impl WriteError {
    /// Whether the task file and its log lines were written, as the task in memory says.
    pub fn files_written(&self) -> bool {
        matches!(self, WriteError::Index(_))
    }

    pub fn into_error(self) -> StoreError {
        match self {
            WriteError::File(error) | WriteError::Log(error) | WriteError::Index(error) => error,
        }
    }
}

#[derive(Default)]
struct SlotState {
    issued: u64,
    done: u64,
    /// The stat of the task file as this service last wrote or read it.
    stat: Option<FileStat>,
}

/// The order of the writes of one task. A write takes a ticket under the store lock and runs when
/// every earlier ticket is done.
#[derive(Default)]
pub struct TaskSlot {
    state: Mutex<SlotState>,
    turn: Condvar,
}

impl TaskSlot {
    fn state(&self) -> MutexGuard<'_, SlotState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, SlotState>) -> MutexGuard<'a, SlotState> {
        let woken = self.turn.wait(state);
        woken.unwrap_or_else(PoisonError::into_inner)
    }

    /// Read under the store lock after `wait_idle`, so no write of this task runs.
    pub fn stat(&self) -> Option<FileStat> {
        self.state().stat
    }

    pub fn set_stat(&self, stat: Option<FileStat>) {
        self.state().stat = stat;
    }

    pub fn ticket(&self) -> u64 {
        let mut state = self.state();
        state.issued += 1;
        state.issued - 1
    }

    /// Waits until every write that has a ticket is done. Under the store lock no new ticket is
    /// taken, and a write never waits for that lock, so this ends.
    pub fn wait_idle(&self) {
        let mut state = self.state();
        while state.done != state.issued {
            state = self.wait(state);
        }
    }

    pub fn wait_turn(&self, ticket: u64) -> Turn<'_> {
        let mut state = self.state();
        while state.done != ticket {
            state = self.wait(state);
        }
        Turn {
            slot: self,
            stat: None,
        }
    }
}

/// A write's turn. It ends when dropped, also after an error, so the next write of the task runs.
pub struct Turn<'a> {
    slot: &'a TaskSlot,
    /// The stat of the file this write made, kept when the turn ends.
    stat: Option<FileStat>,
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        let mut state = self.slot.state();
        if let Some(stat) = self.stat {
            state.stat = Some(stat);
        }
        state.done += 1;
        drop(state);
        self.slot.turn.notify_all();
    }
}

/// The data folder and the index, which every writer shares.
#[derive(Clone)]
pub struct Disk {
    pub data_dir: PathBuf,
    index: Arc<Mutex<TaskIndex>>,
}

impl Disk {
    pub fn new(data_dir: PathBuf, index: TaskIndex) -> Disk {
        Disk {
            data_dir,
            index: Arc::new(Mutex::new(index)),
        }
    }

    /// A failed transaction rolls back when dropped, so a poisoned index is still whole.
    pub fn index(&self) -> MutexGuard<'_, TaskIndex> {
        self.index.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The file, then the event lines, then the index in one transaction: `init` heals a crash
    /// at any point between them. Runs in the task's turn.
    pub fn write(
        &self,
        turn: &mut Turn<'_>,
        task: &Task,
        events: &[LoggedEvent],
        write_file: bool,
    ) -> Result<(), WriteError> {
        let mut files = TaskFiles::default();
        if write_file {
            let stat = write_task_file(&self.data_dir, task)
                .map_err(|error| WriteError::File(error.into()))?;
            files.task = Some(stat);
            turn.stat = Some(stat);
        }
        for logged in events {
            let stat = append_event_line(&self.data_dir, &logged.event, logged.accepted)
                .map_err(|error| WriteError::Log(error.into()))?;
            files.log = Some(stat);
        }
        let change = TaskChange {
            task: Some(task),
            events,
            files,
        };
        self.index()
            .record(&task.id, &change)
            .map_err(|error| WriteError::Index(StoreError::Failed(error)))
    }
}

/// A change already in memory whose files could not be written: memory goes back to `previous`
/// unless a later change replaced `written` there.
pub struct Undo {
    pub written: Task,
    pub previous: Task,
}

/// A write the store decided and left for later, with its ticket.
pub struct WriteJob {
    slot: Arc<TaskSlot>,
    ticket: u64,
    task: Task,
    previous: Option<Task>,
    events: Vec<LoggedEvent>,
    write_file: bool,
    ran: bool,
}

impl WriteJob {
    pub fn new(
        slot: Arc<TaskSlot>,
        task: Task,
        previous: Option<Task>,
        events: Vec<LoggedEvent>,
        write_file: bool,
    ) -> WriteJob {
        let ticket = slot.ticket();
        WriteJob {
            slot,
            ticket,
            task,
            previous,
            events,
            write_file,
            ran: false,
        }
    }

    /// The error, and what to take back from memory when the files were not written.
    pub fn run(mut self, disk: &Disk) -> Result<(), (StoreError, Option<Box<Undo>>)> {
        self.ran = true;
        let mut turn = self.slot.wait_turn(self.ticket);
        let result = disk.write(&mut turn, &self.task, &self.events, self.write_file);
        drop(turn);
        let Err(error) = result else {
            return Ok(());
        };
        if error.files_written() {
            return Err((error.into_error(), None));
        }
        let undo = self.previous.take().map(|previous| Undo {
            written: std::mem::take(&mut self.task),
            previous,
        });
        Err((error.into_error(), undo.map(Box::new)))
    }
}

impl Drop for WriteJob {
    /// A job dropped unrun, as by a panic before it, still ends its turn; else the task's next
    /// write would wait for ever.
    fn drop(&mut self) {
        if !self.ran {
            drop(self.slot.wait_turn(self.ticket));
        }
    }
}

/// The writes one call of the store left for later, to run after it lets the store lock go.
pub struct Writes {
    pub disk: Disk,
    pub jobs: Vec<WriteJob>,
    /// The first error of a write that ran early, under the lock.
    pub failed: Option<StoreError>,
}

/// What `Writes::run` did.
pub struct Ran {
    pub result: Result<(), StoreError>,
    pub undo: Vec<Undo>,
}

impl Writes {
    /// Runs every write in order, also after one fails: each holds a turn of its task.
    pub fn run(self) -> Ran {
        let Writes { disk, jobs, failed } = self;
        let mut ran = Ran {
            result: failed.map_or(Ok(()), Err),
            undo: Vec::new(),
        };
        for job in jobs {
            if let Err((error, undo)) = job.run(&disk) {
                if ran.result.is_ok() {
                    ran.result = Err(error);
                }
                ran.undo.extend(undo.map(|undo| *undo));
            }
        }
        ran
    }
}

#[cfg(test)]
#[path = "task_writes_tests.rs"]
mod tests;
