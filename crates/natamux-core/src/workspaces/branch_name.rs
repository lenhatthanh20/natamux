//! The branch name of a task: the default one, or a project's `git.branchName` pattern filled in.
//! The app fills the Start a task field with it, the daemon names the branch with it.

pub use natamux_proto::projects::settings_rules::{MAX_BRANCH_LENGTH, unknown_placeholder};

use crate::workspaces::slug::slugify;

/// The default: `natamux/<id in lower case>-<slug of the title>`.
pub fn branch_name_for(id: &str, title: &str) -> String {
    format!("natamux/{}-{}", id.to_lowercase(), slugify(title))
}

/// `pattern` with `{id}` and `{slug}` filled in, or the default name when there is none.
pub fn branch_name_from(pattern: Option<&str>, id: &str, title: &str) -> String {
    match pattern {
        Some(pattern) => pattern
            .replace("{id}", id)
            .replace("{slug}", &slugify(title)),
        None => branch_name_for(id, title),
    }
}

/// The rules of `git check-ref-format --branch` that a typed name can break.
pub fn is_branch_name(name: &str) -> bool {
    let bad_char = |c: char| c.is_ascii_control() || " ~^:?*[\\".contains(c);
    !name.is_empty()
        && name.len() <= MAX_BRANCH_LENGTH
        && name != "@"
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.chars().any(bad_char)
        && name
            .split('/')
            .all(|part| !part.starts_with('.') && !part.ends_with(".lock"))
}

#[cfg(test)]
#[path = "branch_name_tests.rs"]
mod tests;
