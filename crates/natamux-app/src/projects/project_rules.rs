//! The Projects dialog: the folder rule, what Add sends, and which field an error belongs to
//! (`repoPathProblem` of `features/projects/issues.ts`, `onAdd` of `RepoPanel.tsx`).

/// How the daemon's errors about the worktree folder start. Its 400 answer names no field.
const WORKTREE_ERROR: &str = "The worktree folder ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoField {
    Folder,
    Worktree,
}

/// A repository folder must be a full path. The daemon checks the rest.
pub fn repo_path_problem(path: &str) -> Option<&'static str> {
    let value = path.trim();
    let bytes = value.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if value.is_empty() || value.starts_with(['/', '~']) || drive || value.starts_with("\\\\") {
        return None;
    }
    Some("Use the full path of the folder, starting with / (or a drive letter on Windows).")
}

/// The folder, the name and the worktree folder to send: the name and the worktree folder only
/// when they are set, and the worktree folder only when it is not the daemon's default.
pub fn add_args(
    path: &str,
    name: &str,
    worktree: &str,
    default_worktree: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    let name = name.trim();
    let own = worktree.trim();
    (
        path.trim().to_owned(),
        (!name.is_empty()).then(|| name.to_owned()),
        (!own.is_empty() && Some(own) != default_worktree).then(|| own.to_owned()),
    )
}

pub fn field_of_error(message: &str) -> RepoField {
    if message.starts_with(WORKTREE_ERROR) {
        RepoField::Worktree
    } else {
        RepoField::Folder
    }
}

/// What the Worktree folder field shows: what was typed, else the daemon's default (which may
/// come after the dialog opened).
pub fn shown_worktree(typed: Option<&str>, default: Option<&str>) -> String {
    typed.or(default).unwrap_or_default().to_owned()
}

#[cfg(test)]
#[path = "project_rules_tests.rs"]
mod tests;
