//! Git that the daemon runs while isolation is on: a worktree's agent can write files the daemon's
//! git would read, so git runs no hook, no fsmonitor command and no `ext::` remote, and skips the
//! system config. Process-wide: isolation is set once, at the daemon's start.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

static EMPTY_HOOKS: OnceLock<PathBuf> = OnceLock::new();

/// Turns the hardening on for the rest of the process. `empty_hooks`: a folder with no hook.
pub fn install_git_hardening(empty_hooks: PathBuf) {
    let _ = EMPTY_HOOKS.set(empty_hooks);
}

/// The arguments that go before git's own, while the hardening is on.
pub fn hardening_args() -> Vec<String> {
    let Some(hooks) = EMPTY_HOOKS.get() else {
        return Vec::new();
    };
    [
        format!("core.hooksPath={}", hooks.display()),
        "core.fsmonitor=false".to_string(),
        "protocol.ext.allow=never".to_string(),
    ]
    .into_iter()
    .flat_map(|entry| ["-c".to_string(), entry])
    .collect()
}

/// Call right after `Command::new("git")`, before any other argument.
pub fn harden_git(command: &mut Command) {
    let args = hardening_args();
    if !args.is_empty() {
        command.args(args).env("GIT_CONFIG_NOSYSTEM", "1");
    }
}
