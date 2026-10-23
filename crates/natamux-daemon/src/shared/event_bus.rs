//! What daemon parts tell each other (`event-bus.ts`). Part 1 of phase 4 has only the terminal
//! messages; the task messages and `/events` come with part 2. Listeners run in the publisher's
//! thread, so a publisher holds no lock when it publishes.

use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DaemonMessage {
    /// A shell of the task started or ended, or the program in its foreground changed.
    ShellsChanged {
        task_id: String,
    },
    TaskDeleted {
        task_id: String,
    },
    /// The worktree of the task was removed: its shells cannot start without it.
    WorkspaceRemoved {
        task_id: String,
    },
}

type Listener = Arc<dyn Fn(&DaemonMessage) + Send + Sync>;

#[derive(Default)]
pub struct EventBus {
    listeners: Mutex<Vec<(u64, Listener)>>,
    next: Mutex<u64>,
}

impl EventBus {
    pub fn new() -> Arc<EventBus> {
        Arc::new(EventBus::default())
    }

    pub fn publish(&self, message: DaemonMessage) {
        let listeners: Vec<Listener> = match self.listeners.lock() {
            Ok(listeners) => listeners
                .iter()
                .map(|(_, listener)| Arc::clone(listener))
                .collect(),
            Err(_) => return,
        };
        for listener in listeners {
            listener(&message);
        }
    }

    /// Gives the id to unsubscribe with.
    pub fn subscribe(&self, listener: impl Fn(&DaemonMessage) + Send + Sync + 'static) -> u64 {
        let id = match self.next.lock() {
            Ok(mut next) => {
                *next += 1;
                *next
            }
            Err(_) => 0,
        };
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.push((id, Arc::new(listener)));
        }
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        if let Ok(mut listeners) = self.listeners.lock() {
            listeners.retain(|(known, _)| *known != id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_every_listener_until_it_unsubscribes() {
        let bus = EventBus::new();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&heard);
        let id = bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
        let changed = DaemonMessage::ShellsChanged {
            task_id: "T-1".into(),
        };
        bus.publish(changed.clone());
        bus.unsubscribe(id);
        bus.publish(changed.clone());
        assert_eq!(*heard.lock().unwrap(), [changed]);
    }
}
