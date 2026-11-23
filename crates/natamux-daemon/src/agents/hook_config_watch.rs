//! Agent config that changed during a session: an agent may load hooks, MCP servers or rules it
//! finds later, so the task's log says so when the agent ends.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use natamux_proto::tasks::task::Task;

use crate::projects::repo_trust_worktree::{TrustState, TrustedOf};

/// The start of the `workspace.warning` the change gives.
pub const CONFIG_CHANGED_DURING: &str =
    "The agent config of this worktree changed while the agent ran";

struct Launched {
    repo: Option<String>,
    workspace: Option<String>,
    digest: Option<String>,
}

/// The digest each running agent started with, by `<task>/<terminal>`.
#[derive(Default)]
pub struct HookConfigWatch {
    launched: Mutex<HashMap<String, Launched>>,
}

fn key(task_id: &str, terminal: &str) -> String {
    format!("{task_id}/{terminal}")
}

impl HookConfigWatch {
    pub fn launched(&self, task: &Task, terminal: &str, digest: Option<&str>) {
        let launch = Launched {
            repo: task.repo.clone(),
            workspace: task.workspace.clone(),
            digest: digest.map(str::to_string),
        };
        if let Ok(mut launched) = self.launched.lock() {
            launched.insert(key(&task.id, terminal), launch);
        }
    }

    /// The agent of a shell ended (or another starts there): the warning text when its config
    /// is not the one it started with.
    pub fn ended(&self, task_id: &str, terminal: &str, trusted_of: &TrustedOf) -> Option<String> {
        let launch = self.launched.lock().ok()?.remove(&key(task_id, terminal))?;
        let workspace = launch.workspace.as_deref().map(Path::new);
        let now = trusted_of(launch.repo.as_deref(), workspace);
        if now.digest().is_some() && now.digest() == launch.digest.as_deref() {
            return None;
        }
        let files = match &now {
            TrustState::Changed { files, .. } if !files.is_empty() => {
                format!(": {}", files.join(", "))
            }
            TrustState::Unverifiable { reason } => format!(": {reason}"),
            _ => String::new(),
        };
        Some(format!("{CONFIG_CHANGED_DURING}{files}."))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn task() -> Task {
        Task {
            id: "T-0001".into(),
            workspace: Some("/w/T-0001".into()),
            ..Task::default()
        }
    }

    #[test]
    fn warns_only_when_the_digest_moved_during_the_session() {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&calls);
        let trusted_of: TrustedOf = Arc::new(move |_: Option<&str>, _: Option<&Path>| {
            let digest = if seen.fetch_add(1, Ordering::SeqCst) == 0 {
                "a"
            } else {
                "b"
            };
            TrustState::Changed {
                digest: digest.into(),
                files: vec![".mcp.json".into()],
            }
        });
        let watch = HookConfigWatch::default();
        watch.launched(&task(), "1", Some("a"));
        assert_eq!(watch.ended("T-0001", "1", &trusted_of), None);
        watch.launched(&task(), "1", Some("a"));
        assert_eq!(
            watch.ended("T-0001", "1", &trusted_of).as_deref(),
            Some("The agent config of this worktree changed while the agent ran: .mcp.json.")
        );
        // Told once per launch.
        assert_eq!(watch.ended("T-0001", "1", &trusted_of), None);
    }
}
