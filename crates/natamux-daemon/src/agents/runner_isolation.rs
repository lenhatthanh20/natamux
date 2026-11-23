//! The isolation side of `AgentRunner`: which agents may start, where their launch scripts go,
//! and the task token that ends with the task's last terminal.

use std::path::PathBuf;
use std::sync::Arc;

use natamux_agents::launch::Platform;

use super::AgentRunner;
use crate::agents::errors::AgentError;
use crate::terminals::isolation::Isolation;

impl AgentRunner {
    pub(super) fn platform(&self) -> Platform {
        if self.options.windows {
            Platform::Windows
        } else {
            Platform::Posix
        }
    }

    /// Isolation for every later start. Set once, at the daemon's start.
    pub fn isolate(&self, isolation: Arc<Isolation>) {
        let _ = self.isolation.set(isolation);
    }

    pub fn isolation(&self) -> Option<Arc<Isolation>> {
        self.isolation.get().cloned()
    }

    /// Under isolation only Claude Code starts, and only where isolation can run: checked
    /// before any file is written or shell started. A custom agent is refused even when it is
    /// based on Claude Code: its command is its own.
    pub(super) fn check_isolation(&self, agent: &str) -> Result<(), AgentError> {
        match self.isolation.get() {
            Some(isolation) => isolation.allows_agent(agent).map_err(AgentError::Untrusted),
            None => Ok(()),
        }
    }

    /// An isolated shell cannot read the data folder, so its scripts go in the task's own
    /// sandbox folder, which it may read.
    pub(super) fn launch_dir_of(&self, task_id: &str) -> PathBuf {
        match self.isolation.get() {
            Some(isolation) => isolation.task_dir(task_id),
            None => self.launch_dir.clone(),
        }
    }

    /// A shell of the task ended. Under isolation the task token is forgotten once no terminal
    /// of the task is left, so a program that kept it reaches nothing; the next shell gets a new
    /// one.
    pub fn forget_token_if_last(&self, task_id: &str) {
        let terminals = &self.options.terminals;
        if self.isolation.get().is_some() && terminals.list(task_id).is_empty() {
            terminals.revoke_agent(task_id);
        }
    }
}
