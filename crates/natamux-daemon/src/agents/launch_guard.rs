//! One gate for every agent start. A new launch, a resume, a restore after a restart, a paused
//! shell that is reused and a reopen all pass `AgentRunner::start`, which calls `guard` before
//! any hook file, trust marker or terminal run. Order: digest D1 → hook files → D2 → launch
//! script → D3 = D2 → run. In Strict mode the script checks the digest once more right before
//! the agent (`natamux repo verify-launch`).

use std::path::Path;

use natamux_agents::launch::{Platform, quote_argument};
use natamux_agents::launch_ask::ask_guaranteed;
use natamux_agents::trust::{TrustPaths, untrust_workspace};
use natamux_core::privacy::audit::record;
use natamux_core::security::{SecurityMode, mode, mode_in};
use natamux_proto::tasks::task::Task;

use crate::agents::errors::AgentError;
use crate::projects::repo_trust_worktree::{TrustState, TrustedOf};

/// What the gate found at D1, for the rest of the start.
#[derive(Clone, Debug)]
pub struct LaunchGate {
    pub state: TrustState,
    pub mode: SecurityMode,
}

fn check(trusted_of: &TrustedOf, task: &Task) -> TrustState {
    trusted_of(
        task.repo.as_deref(),
        task.workspace.as_deref().map(Path::new),
    )
}

/// Takes the worktree out of Cursor's and Copilot's trusted folders, so they ask again.
pub fn drop_trust(task: &Task, paths: &TrustPaths) {
    if let Some(workspace) = &task.workspace {
        untrust_workspace(workspace, paths);
    }
}

/// D1. Strict mode refuses a worktree that is not approved as it is now, and an agent that
/// cannot be held to asking, before anything is written or typed.
pub fn guard(
    trusted_of: &TrustedOf,
    task: &Task,
    kind: &str,
    paths: &TrustPaths,
) -> Result<LaunchGate, AgentError> {
    // The worktree's `.natamux.json` may tighten the user's mode, never loosen it.
    let workspace = task.workspace.as_deref().map(Path::new);
    let mode = workspace.map_or_else(mode, mode_in);
    let state = check(trusted_of, task);
    if let Some(strict) = mode.strict() {
        if !state.is_trusted() {
            drop_trust(task, paths);
            record("agent.blocked", &task.id, "untrusted");
            return Err(AgentError::Untrusted(state.reason()));
        }
        if strict.require_ask_guarantee && !ask_guaranteed(kind) {
            record("agent.blocked", &task.id, "no-ask-mode");
            return Err(AgentError::Untrusted(format!(
                "Strict mode starts only agents that ask before each action, and {kind} has no \
                 such mode."
            )));
        }
    }
    Ok(LaunchGate { state, mode })
}

impl LaunchGate {
    pub fn trusted(&self) -> bool {
        self.state.is_trusted()
    }

    pub fn strict(&self) -> bool {
        self.mode.is_strict()
    }

    /// D2 and D3: an approved start goes on only while the digest is still the approved one.
    /// A start without trust gets nothing from the approval, so it has nothing to lose.
    pub fn recheck(
        &self,
        trusted_of: &TrustedOf,
        task: &Task,
        paths: &TrustPaths,
    ) -> Result<(), AgentError> {
        if !self.trusted() {
            return Ok(());
        }
        let now = check(trusted_of, task);
        if now.is_trusted() && now.digest() == self.state.digest() {
            return Ok(());
        }
        if self.strict() {
            drop_trust(task, paths);
        }
        record("agent.blocked", &task.id, "config-changed");
        Err(AgentError::Untrusted(format!(
            "The agent config changed while the agent started. {}",
            now.reason()
        )))
    }

    /// Strict mode: the digest the launch script checks right before the agent runs.
    pub fn verify_digest(&self) -> Option<&str> {
        self.state.digest().filter(|_| self.strict())
    }

    /// Codex's hook trust bypass: always in Personal mode, in Strict only for an approved digest.
    pub fn bypass_hook_trust(&self) -> bool {
        !self.strict() || self.trusted()
    }
}

/// The command with the pre-exec check in front: the agent runs only when the daemon says the
/// worktree still has the approved digest. `path` is the PATH prefix of the POSIX script.
pub fn with_verify(
    command: &str,
    check: Option<(&str, &str)>,
    path: &str,
    platform: Platform,
) -> String {
    let Some((task_id, digest)) = check else {
        return command.to_string();
    };
    let verify = format!(
        "natamux repo verify-launch --task {} --digest {}",
        quote_argument(task_id, platform),
        quote_argument(digest, platform)
    );
    match platform {
        Platform::Posix => format!("{path}{verify} && {command}"),
        Platform::Windows => format!("{verify}; if ($LASTEXITCODE -eq 0) {{ {command} }}"),
    }
}
