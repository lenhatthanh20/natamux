//! Natamux-written or repo-provided. A file at one of Natamux's hook paths is Natamux's own only
//! when git does not track it, its bytes are exactly what Natamux writes there, and
//! `.git/info/exclude` lists it. Anything else came from the repo (or someone else) and is
//! reviewed like any other agent config file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use natamux_core::js::js_trim;

use crate::hook_files::{COPILOT_EVENTS, WorkspaceHooks, workspace_file};
use crate::launch::LAUNCHABLE_AGENTS;

/// The text Natamux writes at `path` (relative, with `/`), for any agent. Claude Code's file
/// counts as on, since only the daemon writes it.
pub fn natamux_text(path: &str) -> Option<String> {
    let hooks = WorkspaceHooks { claude_code: true };
    LAUNCHABLE_AGENTS
        .iter()
        .filter_map(|agent| workspace_file(agent, hooks))
        .find(|file| file.path == path)
        .map(|file| file.content)
}

/// The exclude file all worktrees of the repository share.
fn exclude_file(worktree: &Path) -> Option<PathBuf> {
    let common = match crate::git_files::common_dir(worktree) {
        Some(common) => common,
        None => {
            let mut git = Command::new("git");
            natamux_core::security::harden_git(&mut git);
            git.args(["rev-parse", "--git-common-dir"])
                .current_dir(worktree)
                .stdin(Stdio::null())
                .stderr(Stdio::null());
            natamux_core::login_path::apply(&mut git);
            let output = git.output().ok().filter(|out| out.status.success())?;
            let text = String::from_utf8_lossy(&output.stdout);
            PathBuf::from(js_trim(&text))
        }
    };
    let common = if common.is_absolute() {
        common
    } else {
        worktree.join(common)
    };
    Some(common.join("info").join("exclude"))
}

/// Whether `.git/info/exclude` has the line `/<path>` that Natamux adds.
pub fn is_excluded(worktree: &Path, path: &str) -> bool {
    let line = format!("/{path}");
    let Some(text) = exclude_file(worktree).and_then(|file| fs::read_to_string(file).ok()) else {
        return false;
    };
    text.lines().any(|each| each == line)
}

/// Whether git tracks `path` in `worktree`. No git, or not a repository: not tracked.
pub fn is_tracked(worktree: &Path, path: &str) -> bool {
    let mut git = Command::new("git");
    natamux_core::security::harden_git(&mut git);
    git.args(["ls-files", "--error-unmatch", "--", path])
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    natamux_core::login_path::apply(&mut git);
    git.status().is_ok_and(|status| status.success())
}

/// An untracked file that Natamux wrote, byte for byte, and keeps out of git. The caller knows
/// it is untracked. A folder outside any repository has no exclude list, and git brought
/// nothing into it, so the exact bytes are enough there.
pub fn is_natamux_file(worktree: &Path, path: &str, bytes: &[u8]) -> bool {
    let exact = natamux_text(path).is_some_and(|text| text.as_bytes() == bytes);
    exact && (exclude_file(worktree).is_none() || is_excluded(worktree, path))
}

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

#[cfg(test)]
#[path = "hook_origin_tests.rs"]
mod tests;
