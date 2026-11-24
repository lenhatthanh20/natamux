//! What daemon parts tell each other and the dashboards (`event-bus.ts`). Messages leave in the
//! order they were queued: a change queues its messages under its own lock and sends them after
//! (`enqueue`, then `flush`), so no listener runs under a lock and two changes made at once keep
//! their order. A message published from a listener goes out after the one being sent.

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use natamux_core::json::to_js_string;
use natamux_core::security::{self, SecurityMode};
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
    /// The security settings changed: saved, or edited outside Natamux (`warning`).
    SecurityChanged {
        strict: bool,
        warning: Option<String>,
    },
    /// Sent to one socket only, which missed messages: load everything.
    Resync,
}

fn task_value(task: &Task) -> Value {
    serde_json::to_value(task).unwrap_or(Value::Null)
}

/// What the agent of the parent sees of a direct subtask, on `GET /tasks/{child}` and `/events`.
pub fn task_view(task: &Task) -> Value {
    json!({
        "id": task.id, "title": task.title, "state": task.state, "stage": task.stage,
        "parentId": task.parent_id, "pr": task.pr, "prStatus": task.pr_status,
        "stageSince": task.stage_since, "branch": task.branch,
    })
}

impl DaemonMessage {
    /// The task the message is about; None for a message about none or many.
    pub fn task_id(&self) -> Option<&str> {
        match self {
            DaemonMessage::TaskCreated { task }
            | DaemonMessage::TaskUpdated { task, .. }
            | DaemonMessage::TaskChanged { task } => Some(&task.id),
            DaemonMessage::TaskDeleted { task_id }
            | DaemonMessage::ActivityChanged { task_id, .. }
            | DaemonMessage::ShellsChanged { task_id }
            | DaemonMessage::ProgramsChanged { task_id, .. }
            | DaemonMessage::WorkspaceDirty { task_id }
            | DaemonMessage::ReviewCommentsChanged { task_id }
            | DaemonMessage::WorkspacePreparing { task_id, .. }
            | DaemonMessage::UsageChanged { task_id, .. } => Some(task_id),
            DaemonMessage::ReposChanged
            | DaemonMessage::JobsChanged { .. }
            | DaemonMessage::LimitsChanged { .. }
            | DaemonMessage::SecurityChanged { .. }
            | DaemonMessage::Resync => None,
        }
    }

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
            DaemonMessage::SecurityChanged { .. } => "security.changed",
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
            DaemonMessage::SecurityChanged { strict, warning } => {
                json!({ "kind": kind, "strict": strict, "warning": warning })
            }
        };
        to_js_string(&value)
    }

    /// The frame of a task message for the agent of the parent: the kind and the task's short
    /// view only, never the event, as `GET /tasks/{child}` answers.
    pub fn to_child_json(&self) -> String {
        match self {
            DaemonMessage::TaskCreated { task }
            | DaemonMessage::TaskUpdated { task, .. }
            | DaemonMessage::TaskChanged { task } => {
                to_js_string(&json!({ "kind": self.kind(), "task": task_view(task) }))
            }
            _ => self.to_json(),
        }
    }
}

/// Which messages a client on /events gets (F6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientScope {
    /// The desktop: every message.
    All,
    /// The agent token of this task: its own messages, and task messages of its direct subtasks.
    Task(String),
}

impl ClientScope {
    pub fn allows(&self, message: &DaemonMessage) -> bool {
        let ClientScope::Task(own) = self else {
            return true;
        };
        match message {
            DaemonMessage::TaskCreated { task }
            | DaemonMessage::TaskUpdated { task, .. }
            | DaemonMessage::TaskChanged { task } => {
                task.id == *own || task.parent_id.as_deref() == Some(own.as_str())
            }
            _ => message.task_id() == Some(own.as_str()),
        }
    }

    /// The message is about a direct subtask: the scope gets its short view only.
    fn sees_a_child(&self, message: &DaemonMessage) -> bool {
        let ClientScope::Task(own) = self else {
            return false;
        };
        match message {
            DaemonMessage::TaskCreated { task }
            | DaemonMessage::TaskUpdated { task, .. }
            | DaemonMessage::TaskChanged { task } => task.id != *own,
            _ => false,
        }
    }
}

type Listener = Arc<dyn Fn(&DaemonMessage) + Send + Sync>;
/// A dashboard on /events gets the message as JSON text.
type TextListener = Arc<dyn Fn(&str) + Send + Sync>;
/// Ends a socket an agent token opened, once that token is revoked.
type End = Box<dyn FnOnce() + Send>;

#[derive(Default)]
pub struct EventBus {
    listeners: Mutex<Vec<(u64, Listener)>>,
    clients: Mutex<Vec<(u64, ClientScope, TextListener)>>,
    /// By task: what ends the sockets its agent token opened.
    ends: Mutex<Vec<(u64, String, End)>>,
    next: AtomicU64,
    /// Each message with the security mode a test forced on the thread that queued it.
    outbox: Mutex<VecDeque<(DaemonMessage, Option<SecurityMode>)>>,
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
            outbox.push_back((message, security::forced()));
        }
    }

    /// Sends the queued messages, unless another thread sends them now.
    pub fn flush(&self) {
        loop {
            let Ok(_sending) = self.sending.try_lock() else {
                return;
            };
            while let Some((message, forced)) = self.pop() {
                // Another thread may send it: the listeners see the mode of the one that queued
                // it, as in `StageRun::background`.
                let _mode = forced.map(security::force);
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

    fn pop(&self) -> Option<(DaemonMessage, Option<SecurityMode>)> {
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
        let clients: Vec<(bool, TextListener)> = self.clients.lock().map_or_else(
            |_| Vec::new(),
            |list| {
                list.iter()
                    .filter(|(_, scope, _)| scope.allows(message))
                    .map(|(_, scope, each)| (scope.sees_a_child(message), Arc::clone(each)))
                    .collect()
            },
        );
        if clients.is_empty() {
            return;
        }
        // Each text is made once per message for all dashboards.
        let (mut full, mut child) = (None, None);
        for (sees_a_child, client) in clients {
            let text: &str = if sees_a_child {
                child.get_or_insert_with(|| message.to_child_json())
            } else {
                full.get_or_insert_with(|| message.to_json())
            };
            if catch_unwind(AssertUnwindSafe(|| client(text))).is_err() {
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
            clients.retain(|(known, _, _)| *known != id);
        }
        if let Ok(mut ends) = self.ends.lock() {
            ends.retain(|(known, _, _)| *known != id);
        }
    }

    /// `end` runs when the agent token of `task_id` is revoked (`end_agent`), so a socket that
    /// token opened closes. `unsubscribe` with the id drops it unrun.
    pub fn on_agent_end(&self, task_id: &str, end: impl FnOnce() + Send + 'static) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut ends) = self.ends.lock() {
            ends.push((id, task_id.to_string(), Box::new(end)));
        }
        id
    }

    /// The agent token of `task_id` was revoked: its `/events` clients get no more messages, and
    /// every socket it opened ends. Desktop clients stay.
    pub fn end_agent(&self, task_id: &str) {
        let scope = ClientScope::Task(task_id.to_string());
        if let Ok(mut clients) = self.clients.lock() {
            clients.retain(|(_, each, _)| *each != scope);
        }
        let ended: Vec<End> = match self.ends.lock() {
            Ok(mut ends) => {
                let (gone, kept): (Vec<_>, Vec<_>) =
                    ends.drain(..).partition(|(_, task, _)| task == task_id);
                *ends = kept;
                gone.into_iter().map(|(_, _, end)| end).collect()
            }
            Err(_) => Vec::new(),
        };
        // Outside the lock: an end may unsubscribe.
        for end in ended {
            end();
        }
    }

    /// A client on /events: it gets each message its scope allows as JSON text.
    pub fn subscribe_client(
        &self,
        scope: ClientScope,
        listener: impl Fn(&str) + Send + Sync + 'static,
    ) -> u64 {
        let id = self.next.fetch_add(1, Ordering::SeqCst) + 1;
        if let Ok(mut clients) = self.clients.lock() {
            clients.push((id, scope, Arc::new(listener)));
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
