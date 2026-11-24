//! Checks on hook commands and files that both the writer (`hook_files`) and the origin test
//! (`hook_origin`) need, kept apart so the two do not import each other.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::launch::LAUNCHABLE_AGENTS;

/// Copilot CLI does not tell a hook which event it is, so each event names itself.
pub(crate) const COPILOT_EVENTS: [&str; 7] = [
    "userPromptSubmitted",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "notification",
    "agentStop",
    "errorOccurred",
];

/// Exactly `natamux hook <agent>`, or Copilot's `natamux hook copilot <event>`: anything after
/// it (`; evil`, a pipe) would run too.
pub(crate) fn is_hook_command(text: &str) -> bool {
    let words: Vec<&str> = text.split(' ').collect();
    match words.as_slice() {
        ["natamux", "hook", "copilot", event] => COPILOT_EVENTS.contains(event),
        ["natamux", "hook", agent] => LAUNCHABLE_AGENTS.contains(agent),
        _ => false,
    }
}

/// The only top-level keys of a hook file Natamux writes. Any other (`env`, `apiKeyHelper`,
/// `mcpServers`, `permissions`, …) makes the file someone else's.
pub(crate) const HOOK_FILE_KEYS: [&str; 4] = ["hooks", "statusLine", "ui", "version"];

/// Whether git tracks `path` in `worktree`. No git, or not a repository: not tracked.
pub fn is_tracked(worktree: &Path, path: &str) -> bool {
    let mut git = Command::new("git");
    natamux_core::security::harden_git_at(&mut git, worktree);
    git.args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    natamux_core::login_path::apply(&mut git);
    git.status().is_ok_and(|status| status.success())
}
