//! `dirty-tracker.ts`: tells dashboards that the files of a workspace may have changed.
//!
//! Hooks and output come in bursts: dashboards hear of a change in a task this long after it
//! (`DIRTY_DEBOUNCE_MS`). Each message makes them read git again. A few in a row go out soon, so a
//! user's command in a new terminal shows at once; after that, one per `DIRTY_MIN_INTERVAL_MS`
//! while changes go on, so a busy agent costs no more than the old 10 s poll. `touch` only marks
//! and plans a timer: it runs on the output path and never calls git.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::scheduler::{Scheduler, TimerId};
use crate::tasks::service::TaskService;

pub const DIRTY_DEBOUNCE_MS: u64 = 750;
pub const DIRTY_BURST: u32 = 3;
pub const DIRTY_MIN_INTERVAL_MS: u64 = 10_000;

/// The `workspace.dirty` messages of one task. `credits`: how many may go out soon; one comes back
/// per `DIRTY_MIN_INTERVAL_MS`, up to `DIRTY_BURST`. `again`: a touch came that no message told
/// yet.
struct DirtyState {
    credits: u32,
    again: bool,
    tell: Option<TimerId>,
    refill: Option<TimerId>,
}

struct Inner {
    tasks: TaskService,
    bus: Option<Arc<EventBus>>,
    scheduler: Arc<Scheduler>,
    dirty: Mutex<HashMap<String, DirtyState>>,
}

pub struct DirtyTracker {
    inner: Arc<Inner>,
}

impl DirtyTracker {
    pub fn new(
        tasks: TaskService,
        bus: Option<Arc<EventBus>>,
        scheduler: Arc<Scheduler>,
    ) -> DirtyTracker {
        DirtyTracker {
            inner: Arc::new(Inner {
                tasks,
                bus,
                scheduler,
                dirty: Mutex::default(),
            }),
        }
    }

    pub fn touch(&self, task_id: &str) {
        let inner = &self.inner;
        if inner.bus.is_none() {
            return;
        }
        let mut dirty = inner.lock();
        let state = dirty.entry(task_id.to_string()).or_insert(DirtyState {
            credits: DIRTY_BURST,
            again: false,
            tell: None,
            refill: None,
        });
        state.again = true;
        inner.plan(task_id, state);
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, DirtyState>> {
        self.dirty
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn plan(self: &Arc<Self>, task_id: &str, state: &mut DirtyState) {
        if state.tell.is_some() || state.credits == 0 || !state.again {
            return;
        }
        let me = Arc::clone(self);
        let id = task_id.to_string();
        state.tell = Some(
            self.scheduler
                .after(DIRTY_DEBOUNCE_MS, move || me.tell(&id)),
        );
    }

    fn tell(self: &Arc<Self>, task_id: &str) {
        // Asked before the lock: the task service has its own.
        let exists = self.tasks.get_task(task_id).is_ok();
        {
            let mut dirty = self.lock();
            let Some(state) = dirty.get_mut(task_id) else {
                return;
            };
            state.tell = None;
            if !exists {
                // Deleted meanwhile.
                self.scheduler.clear(&mut state.refill);
                dirty.remove(task_id);
                return;
            }
            state.again = false;
            state.credits = state.credits.saturating_sub(1);
            self.refill(task_id, state);
        }
        // After the lock: a listener may touch again.
        if let Some(bus) = &self.bus {
            bus.publish(DaemonMessage::WorkspaceDirty {
                task_id: task_id.to_string(),
            });
        }
    }

    fn refill(self: &Arc<Self>, task_id: &str, state: &mut DirtyState) {
        if state.refill.is_some() {
            return;
        }
        let me = Arc::clone(self);
        let id = task_id.to_string();
        state.refill = Some(
            self.scheduler
                .after(DIRTY_MIN_INTERVAL_MS, move || me.refilled(&id)),
        );
    }

    fn refilled(self: &Arc<Self>, task_id: &str) {
        let mut dirty = self.lock();
        let Some(state) = dirty.get_mut(task_id) else {
            return;
        };
        state.refill = None;
        state.credits += 1;
        if state.credits < DIRTY_BURST {
            self.refill(task_id, state);
        } else if state.tell.is_none() && !state.again {
            dirty.remove(task_id);
            return;
        }
        if let Some(state) = dirty.get_mut(task_id) {
            self.plan(task_id, state);
        }
    }
}
