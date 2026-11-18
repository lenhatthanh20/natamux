//! The reads of a task's events. Each waits first for the writes of the task decided so far, so
//! it sees their events. A read a hook makes on every call does that wait after the store lock,
//! so the changes of the other tasks do not wait behind one task's disk.

use std::sync::Arc;

use natamux_proto::events::task_event::StoredEvent;

use super::TaskSync;
use crate::task_writes::{Disk, TaskSlot};

/// A read of a task's accepted `stage.started` events, made under the store lock and run after
/// it. Every change decided before it was made took its ticket under that lock, so the read waits
/// for it and sees its events.
pub struct StageStarts {
    disk: Disk,
    /// None for a task with no write so far.
    slot: Option<Arc<TaskSlot>>,
    task_id: String,
}

impl StageStarts {
    /// Oldest first. Run with the store lock let go, and never by a call that still holds writes
    /// of its own. A change decided after the read was made may be in too, as for a read that
    /// came a moment later. It only reads, so it can lose no change.
    pub fn read(self) -> Result<Vec<StoredEvent>, String> {
        if let Some(slot) = &self.slot {
            slot.wait_idle();
        }
        self.disk.index().stage_starts(&self.task_id)
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
        // The writes this call left for later hold tickets: the read must not wait for them.
        self.run_later();
        StageStarts {
            disk: self.disk.clone(),
            slot: self.slots.get(task_id).cloned(),
            task_id: task_id.to_string(),
        }
    }
}
