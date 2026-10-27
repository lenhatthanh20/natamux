//! What daemon parts tell each other and the dashboards (`event-bus.ts`). Messages leave in the
//! order they were queued: a change queues its messages under its own lock and sends them after
//! (`enqueue`, then `flush`), so no listener runs under a lock and two changes made at once keep
//! their order. A message published from a listener goes out after the one being sent.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use natamux_core::json::to_js_string;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::Task;
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, PartialEq)]
pub enum DaemonMessage {
    TaskCreated {
        task: Task,
    },
    /// `moved`: the task entered another step; left out by writers that never move it.
    TaskUpdated {
        task: Task,
        event: TaskEvent,
        accepted: bool,
        moved: Option<bool>,
    },
    /// A change that is not worth a line in the log.
    TaskChanged {
        task: Task,
    },
    /// The task and its files are gone.
    TaskDeleted {
        task_id: String,
    },
    ReposChanged,
    /// The task's whole activity after the change, by terminal.
    ActivityChanged {
        task_id: String,
        activity: Map<String, Value>,
    },
    /// A shell of the task started or ended, or the program in its foreground changed.
    ShellsChanged {
        task_id: String,
    },
    /// The program in the foreground of these shells changed; none opened or closed. Goes out as
    /// `shells.changed`: the names are for `OpenShells`, which looks at only those shells.
    ProgramsChanged {
        task_id: String,
        terminals: Vec<String>,
    },
    WorkspaceDirty {
        task_id: String,
    },
    /// A review comment of the task was added, changed, sent or deleted, or its lines moved.
    ReviewCommentsChanged {
        task_id: String,
    },
    /// The background jobs that run now.
    JobsChanged {
        jobs: Vec<Job>,
    },
    /// The step a new workspace is at; None: it ended, with `error` when it failed.
    WorkspacePreparing {
        task_id: String,
        step: Option<String>,
        error: Option<String>,
    },
    UsageChanged {
        task_id: String,
        usage: Value,
    },
    /// The plan limits of every agent that told them, as in `GET /limits`.
    LimitsChanged {
        limits: Value,
    },
    /// Sent to one socket only, which missed messages: load everything.
    Resync,
}

fn task_value(task: &Task) -> Value {
    serde_json::to_value(task).unwrap_or(Value::Null)
}

impl DaemonMessage {
    pub fn kind(&self) -> &'static str {
        match self {
            DaemonMessage::TaskCreated { .. } => "task.created",
            DaemonMessage::TaskUpdated { .. } => "task.updated",
            DaemonMessage::TaskChanged { .. } => "task.changed",
            DaemonMessage::TaskDeleted { .. } => "task.deleted",
            DaemonMessage::ReposChanged => "repos.changed",
            DaemonMessage::ActivityChanged { .. } => "activity.changed",
            DaemonMessage::ShellsChanged { .. } | DaemonMessage::ProgramsChanged { .. } => {
                "shells.changed"
            }
            DaemonMessage::WorkspaceDirty { .. } => "workspace.dirty",
            DaemonMessage::ReviewCommentsChanged { .. } => "review-comments.changed",
            DaemonMessage::JobsChanged { .. } => "jobs.changed",
            DaemonMessage::WorkspacePreparing { .. } => "workspace.preparing",
            DaemonMessage::UsageChanged { .. } => "usage.changed",
            DaemonMessage::LimitsChanged { .. } => "limits.changed",
            DaemonMessage::Resync => "resync",
        }
    }

    /// `JSON.stringify` of the Node daemon's object, keys in its order.
    pub fn to_json(&self) -> String {
        let kind = self.kind();
        let value = match self {
            DaemonMessage::TaskCreated { task } | DaemonMessage::TaskChanged { task } => {
                json!({ "kind": kind, "task": task_value(task) })
            }
            DaemonMessage::TaskUpdated {
                task,
                event,
                accepted,
                moved,
            } => {
                let mut value = json!({
                    "kind": kind, "task": task_value(task),
                    "event": serde_json::to_value(event).unwrap_or(Value::Null), "accepted": accepted,
                });
                if let (Some(moved), Some(map)) = (moved, value.as_object_mut()) {
                    map.insert("moved".into(), json!(moved));
                }
                value
            }
            DaemonMessage::TaskDeleted { task_id }
            | DaemonMessage::ShellsChanged { task_id }
            | DaemonMessage::ProgramsChanged { task_id, .. }
            | DaemonMessage::ReviewCommentsChanged { task_id }
            | DaemonMessage::WorkspaceDirty { task_id } => {
                json!({ "kind": kind, "taskId": task_id })
            }
            DaemonMessage::ReposChanged | DaemonMessage::Resync => json!({ "kind": kind }),
            DaemonMessage::ActivityChanged { task_id, activity } => {
                json!({ "kind": kind, "taskId": task_id, "activity": activity })
            }
            DaemonMessage::JobsChanged { jobs } => json!({ "kind": kind, "jobs": jobs }),
            DaemonMessage::WorkspacePreparing {
                task_id,
                step,
                error,
            } => {
                let mut value = json!({ "kind": kind, "taskId": task_id, "step": step });
                if let (Some(error), Some(map)) = (error, value.as_object_mut()) {
                    map.insert("error".into(), json!(error));
                }
                value
            }
            DaemonMessage::UsageChanged { task_id, usage } => {
                json!({ "kind": kind, "taskId": task_id, "usage": usage })
            }
            DaemonMessage::LimitsChanged { limits } => json!({ "kind": kind, "limits": limits }),
        };
        to_js_string(&value)
    }
}

type Listener = Arc<dyn Fn(&DaemonMessage) + Send + Sync>;
/// A dashboard on /events gets the message as JSON text.
type TextListener = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Default)]
pub struct EventBus {
    listeners: Mutex<Vec<(u64, Listener)>>,
    clients: Mutex<Vec<(u64, TextListener)>>,
    next: AtomicU64,
    outbox: Mutex<VecDeque<DaemonMessage>>,
    sending: Mutex<()>,
}

impl EventBus {
    pub fn new() -> Arc<EventBus> {
        Arc::new(EventBus::default())
    }

    /// Queues and sends at once.
    pub fn publish(&self, message: DaemonMessage) {
        self.enqueue(message);
        self.flush();
    }

    /// Queues a message; it goes out at the next `flush`, after every message queued before it.
    pub fn enqueue(&self, message: DaemonMessage) {
        if let Ok(mut outbox) = self.outbox.lock() {
            outbox.push_back(message);
        }
    }

    /// Sends the queued messages, unless another thread sends them now.
    pub fn flush(&self) {
        loop {
            let Ok(_sending) = self.sending.try_lock() else {
                return;
            };
            while let Some(message) = self.pop() {
                self.deliver(&message);
            }
            drop(_sending);
            // A message queued while the lock was let go would wait for the next flush.
            if self.outbox.lock().map_or(true, |outbox| outbox.is_empty()) {
                return;
            }
        }
    }

    /// Sends the queued messages and waits while another thread sends them, so a change this
    /// thread made has reached every listener when it returns, as on Node's one thread. The stage
    /// runner needs it: a move it applies must start the next stage before it answers. Never call
    /// it from a listener.
    pub fn deliver_all(&self) {
        loop {
            self.flush();
            let idle = self.sending.try_lock().is_ok();
            if idle && self.outbox.lock().map_or(true, |outbox| outbox.is_empty()) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_micros(100));
        }
    }

    fn pop(&self) -> Option<DaemonMessage> {
        self.outbox.lock().ok()?.pop_front()
    }

    fn deliver(&self, message: &DaemonMessage) {
        let listeners: Vec<Listener> = self.listeners.lock().map_or_else(
            |_| Vec::new(),
            |list| list.iter().map(|(_, each)| Arc::clone(each)).collect(),
        );
        for listener in listeners {
            // A failing listener must not stop the others.
            if catch_unwind(AssertUnwindSafe(|| listener(message))).is_err() {
                eprintln!("A listener failed on {}", message.kind());
            }
        }
        let clients: Vec<TextListener> = self.clients.lock().map_or_else(
            |_| Vec::new(),
            |list| list.iter().map(|(_, each)| Arc::clone(each)).collect(),
        );
        if clients.is_empty() {
            return;
        }
        // The text is made once per message for all dashboards.
        let text = message.to_json();
        for client in clients {
            if catch_unwind(AssertUnwindSafe(|| client(&text))).is_err() {
                eprintln!("A dashboard failed on {}", message.kind());
            }
        }
    }

    /// Gives the id to unsubscribe with.
    pub fn subscribe(&self, listener: impl Fn(&DaemonMessage) + Send + Sync + 'static) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.push((id, Arc::new(listener)));
        }
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.retain(|(known, _)| *known != id);
        }
        if let Ok(mut clients) = self.clients.lock() {
            clients.retain(|(known, _)| *known != id);
        }
    }

    /// A dashboard: it gets each message as JSON text.
    pub fn subscribe_client(&self, listener: impl Fn(&str) + Send + Sync + 'static) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut clients) = self.clients.lock() {
            clients.push((id, Arc::new(listener)));
        }
        id
    }

    /// How many dashboards listen.
    pub fn size(&self) -> usize {
        self.clients.lock().map_or(0, |clients| clients.len())
    }
}

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;
