//! The ask guarantee of Strict mode: each agent starts in a mode where it asks before it acts,
//! or it does not start. Flags as each CLI documents them (checked 2026-11-23).

use natamux_core::security::mode;

use crate::launch_platform::Platform;

/// How an agent is held to asking before each action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Words on its command line, before the user's own.
    Flags(&'static [&'static str]),
    /// A variable set for its command (Goose has no flag for its mode).
    Env(&'static str, &'static str),
    /// No documented way: blocked while the guarantee is on.
    Blocked,
}

/// Adapters with no documented ask mode, and any other CLI.
const BLOCKED: [&str; 12] = [
    "copilot", "cursor", "opencode", "droid", "amp", "kimi", "aider", "crush", "cline", "kiro",
    "muse", "other",
];

/// The row of every adapter, `other` included. None only for a name with no row, which the
/// exhaustiveness test catches.
pub fn ask_row(kind: &str) -> Option<Ask> {
    Some(match kind {
        "claude-code" => Ask::Flags(&["--permission-mode", "default"]),
        "codex" => Ask::Flags(&[
            "--ask-for-approval",
            "on-request",
            "--sandbox",
            "workspace-write",
        ]),
        "gemini" | "qwen" => Ask::Flags(&["--approval-mode", "default"]),
        "goose" => Ask::Env("GOOSE_MODE", "approve"),
        kind if BLOCKED.contains(&kind) => Ask::Blocked,
        _ => return None,
    })
}

/// Whether Natamux can hold this agent to asking.
pub fn ask_guaranteed(kind: &str) -> bool {
    matches!(ask_row(kind), Some(Ask::Flags(_) | Ask::Env(..)))
}

/// The words that put the agent in its ask mode; empty for the others.
pub fn ask_flags(kind: &str) -> Vec<String> {
    match ask_row(kind) {
        Some(Ask::Flags(flags)) => flags.iter().map(|flag| (*flag).to_string()).collect(),
        _ => Vec::new(),
    }
}

/// The command line with the agent's ask variable set in front, when it has one.
pub fn with_ask_env(kind: &str, command: &str, platform: Platform) -> String {
    match (ask_row(kind), platform) {
        (Some(Ask::Env(name, value)), Platform::Posix) => format!("{name}={value} {command}"),
        (Some(Ask::Env(name, value)), Platform::Windows) => {
            format!("$env:{name}='{value}'; {command}")
        }
        _ => command.to_string(),
    }
}

/// Whether the agent may start now: in Strict mode with the guarantee on, only an agent that
/// can be held to asking.
pub fn allowed_now(kind: &str) -> bool {
    match mode().strict() {
        Some(strict) if strict.require_ask_guarantee => ask_guaranteed(kind),
        _ => true,
    }
}

#[cfg(test)]
#[path = "launch_ask_tests.rs"]
mod tests;
