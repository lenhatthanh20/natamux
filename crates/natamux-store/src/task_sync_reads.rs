//! The reads of a task's events. Each waits first for the writes of the task decided so far, so
//! it sees their events. A read a hook makes on every call does that wait after the store lock,
//! so the changes of the other tasks do not wait behind one task's disk.

use std::sync::Arc;

use natamux_proto::events::task_event::StoredEvent;

use super::TaskSync;
use crate::task_writes::{Disk, TaskSlot};

/// The writes of a task decided when a read was made, and the index to read once they are done.
struct Settled {
    disk: Disk,
    /// The task's slot and its tickets so far. None for a task with no write so far.
    writes: Option<(Arc<TaskSlot>, u64)>,
    task_id: String,
}

impl Settled {
    /// Waits for the writes decided when the read was made, not for later ones: a task that is
    /// written all the time must not hold its reader back.
    fn wait(&self) {
        if let Some((slot, issued)) = &self.writes {
            slot.wait_done(*issued);
        }
    }
}

/// A read of a task's accepted `stage.started` events, made under the store lock and run after
/// it. Every change decided before it was made took its ticket under that lock, so the read waits
/// for it and sees its events.
pub struct StageStarts(Settled);

impl StageStarts {
    /// Oldest first. Run with the store lock let go, and never by a call that still holds writes
    /// of its own. A change decided after the read was made may be in too, as for a read that
    /// came a moment later. It only reads, so it can lose no change.
    pub fn read(self) -> Result<Vec<StoredEvent>, String> {
        self.0.wait();
        self.0.disk.index().stage_starts(&self.0.task_id)
    }
}

/// A read of the end of a task's events, made and run as `StageStarts`.
pub struct LastEvents(Settled);

impl LastEvents {
    /// The last `limit` events, oldest first. May run again with a larger limit.
    pub fn read(&self, limit: usize) -> Result<Vec<StoredEvent>, String> {
        self.0.wait();
        self.0.disk.index().last_events(&self.0.task_id, limit)
    }
}

impl TaskSync {
    /// The events of a task, oldest first, with those of every change decided so far.
    pub fn list_events(&mut self, task_id: &str) -> Result<Vec<StoredEvent>, String> {
        self.settle(task_id);
        self.index().list_events(task_id)
    }

    /// The accepted `stage.started` events of `list_events`, read under the lock: for a change
    /// that is decided from them.
    pub fn stage_starts(&mut self, task_id: &str) -> Result<Vec<StoredEvent>, String> {
        self.settle(task_id);
        self.index().stage_starts(task_id)
    }

    /// The same events as a read to run after the lock.
    pub fn stage_starts_later(&mut self, task_id: &str) -> StageStarts {
        StageStarts(self.settled(task_id))
    }

    /// The end of `list_events` as a read to run after the lock.
    pub fn last_events_later(&mut self, task_id: &str) -> LastEvents {
        LastEvents(self.settled(task_id))
    }

    fn settled(&mut self, task_id: &str) -> Settled {
        // The writes this call left for later hold tickets: the read must not wait for them.
        self.run_later();
        let slot = self.slots.get(task_id);
        Settled {
            disk: self.disk.clone(),
            writes: slot.map(|slot| (Arc::clone(slot), slot.issued())),
            task_id: task_id.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "task_sync_reads_tests.rs"]
mod tests;
