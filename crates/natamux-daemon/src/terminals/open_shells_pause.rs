//! Paused shells: the daemon ended the idle agent of a shell that no one looked at. The shell
//! waits like one after a restart, keeps its agent and session, and comes back when its task is
//! opened. Unlike the others, it does not come back when the daemon starts.

use super::open_shells::{OpenShells, key_of, resumable};

impl OpenShells {
    /// Marks an open shell paused before the daemon closes it: it waits with its agent and
    /// session, and its quit watch stops, so the close is not taken for a quit. False when the
    /// shell is not open or has no agent with a session to come back on.
    pub fn pause(&self, task_id: &str, terminal: &str) -> bool {
        let key = key_of(task_id, terminal);
        {
            let mut state = self.lock();
            let resumes = state
                .entries
                .get(task_id)
                .and_then(|shells| shells.get(terminal))
                .is_some_and(|entry| resumable(entry).is_some() && entry.session.is_some());
            if state.stopped || !resumes || !self.options.terminals.is_open(task_id, terminal) {
                return false;
            }
            let waiting = state.waiting.entry(task_id.into()).or_default();
            if !waiting.iter().any(|name| name == terminal) {
                waiting.push(terminal.into());
            }
            state.paused.insert(key.clone());
            state.pausing.insert(key.clone());
            state.active.remove(&key);
            if let Some(timer) = state.exit_timers.remove(&key) {
                self.options.scheduler.cancel(timer);
            }
            self.write(&state, task_id);
        }
        true
    }

    /// The paused shell is closed: a shell of its name from now on is a new one.
    pub fn pause_done(&self, task_id: &str, terminal: &str) {
        self.lock().pausing.remove(&key_of(task_id, terminal));
        self.publish_changed(task_id);
    }

    /// A pause whose shell could not be closed: it runs on as before.
    pub fn unpause(&self, task_id: &str, terminal: &str) {
        let key = key_of(task_id, terminal);
        {
            let mut state = self.lock();
            state.pausing.remove(&key);
            if state.paused.remove(&key) {
                if let Some(waiting) = state.waiting.get_mut(task_id) {
                    waiting.retain(|name| name != terminal);
                }
                state.active.insert(key);
                self.write(&state, task_id);
            }
        }
        self.publish_changed(task_id);
    }

    pub fn is_paused(&self, task_id: &str, terminal: &str) -> bool {
        self.lock().paused.contains(&key_of(task_id, terminal))
    }

    /// The agent that runs in a shell, as far as the list knows, when the daemon can start it.
    pub fn agent_of(&self, task_id: &str, terminal: &str) -> Option<String> {
        self.lock()
            .entries
            .get(task_id)
            .and_then(|shells| shells.get(terminal))
            .and_then(resumable)
            .map(String::from)
    }
}
