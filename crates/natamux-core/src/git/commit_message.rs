//! Ports `packages/core/src/git/commit-message.ts`: the first line of a commit message, and the
//! simple draft for when no agent can write one.

use natamux_proto::workspaces::changes::{COMMIT_SUBJECT_MAX, ChangeStatus};
use natamux_proto::workspaces::workspace::ChangeEntry;

use crate::js::{is_js_space, utf16_len};

const MAX_LISTED_FILES: usize = 20;

/// Cuts a first line to the limit, at the last space when there is one. The limit counts UTF-16
/// units as JS does; a cut never splits a character (JS could leave half of one).
pub fn cut_commit_subject(line: &str) -> String {
    if utf16_len(line) <= COMMIT_SUBJECT_MAX {
        return line.to_string();
    }
    let mut units = 0;
    let mut last_space = None;
    let mut limit_end = 0;
    for (byte, c) in line.char_indices() {
        if units > COMMIT_SUBJECT_MAX {
            break;
        }
        if c == ' ' && units > 0 {
            last_space = Some(byte);
        }
        if units + c.len_utf16() <= COMMIT_SUBJECT_MAX {
            limit_end = byte + c.len_utf8();
        }
        units += c.len_utf16();
    }
    line[..last_space.unwrap_or(limit_end)]
        .trim_end_matches(is_js_space)
        .to_string()
}

fn one_file(change: &ChangeEntry) -> String {
    match change.status {
        ChangeStatus::Added | ChangeStatus::Untracked => format!("Add {}", change.path),
        ChangeStatus::Deleted => format!("Delete {}", change.path),
        ChangeStatus::Renamed => format!(
            "Rename {} to {}",
            change.original_path.as_deref().unwrap_or(&change.path),
            change.path
        ),
        _ => format!("Update {}", change.path),
    }
}

/// A message from the task title and the changed files, for when no agent can write one.
pub fn simple_commit_draft(title: &str, changes: &[ChangeEntry]) -> String {
    if let [only] = changes {
        return cut_commit_subject(&one_file(only));
    }
    let title = crate::js::js_trim(title);
    let subject = cut_commit_subject(&if title.is_empty() {
        format!("Update {} files", changes.len())
    } else {
        title.to_string()
    });
    let mut lines: Vec<String> = changes
        .iter()
        .take(MAX_LISTED_FILES)
        .map(|change| match (&change.status, &change.original_path) {
            (ChangeStatus::Renamed, Some(original)) => {
                format!("R {original} -> {}", change.path)
            }
            _ => format!("{} {}", change.status.letter(), change.path),
        })
        .collect();
    if changes.len() > MAX_LISTED_FILES {
        lines.push(format!(
            "and {} more files",
            changes.len() - MAX_LISTED_FILES
        ));
    }
    if lines.is_empty() {
        subject
    } else {
        format!("{subject}\n\n{}", lines.join("\n"))
    }
}

#[cfg(test)]
#[path = "commit_message_tests.rs"]
mod tests;
