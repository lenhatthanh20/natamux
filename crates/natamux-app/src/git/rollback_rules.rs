//! The choices of the rollback dialog (`features/git/RollbackDialog.tsx`) and what offers a
//! rollback (`ChangedFiles.tsx` row menu). Pure, so the dialog only draws what this says.

// The rollback dialog uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::BTreeSet;

use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::ChangeEntry;

use crate::git::commit_rules::files_text;

pub const CONFLICT_REASON: &str = "Resolve the conflict first";
pub const NOTHING_CHECKED_REASON: &str = "Check a file first";
pub const BUSY_REASON: &str = "Another git action is running";

/// The state of the "All files" box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    Checked,
    Mixed,
    Unchecked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllState {
    pub tri: Tri,
    /// Nothing can be checked: every file is a conflict, or there is none.
    pub off: bool,
}

fn is_conflict(entry: &ChangeEntry) -> bool {
    entry.status == ChangeStatus::Conflict
}

fn open(files: &[ChangeEntry]) -> impl Iterator<Item = &ChangeEntry> {
    files.iter().filter(|entry| !is_conflict(entry))
}

/// All files are checked at first, except a conflict, which has to be resolved instead.
pub fn first_checks(files: &[ChangeEntry]) -> BTreeSet<String> {
    open(files).map(|entry| entry.path.clone()).collect()
}

/// The checked files that can be rolled back, in the order of the list.
pub fn picked(files: &[ChangeEntry], checked: &BTreeSet<String>) -> Vec<String> {
    open(files)
        .filter(|entry| checked.contains(&entry.path))
        .map(|entry| entry.path.clone())
        .collect()
}

pub fn all_state(files: &[ChangeEntry], checked: &BTreeSet<String>) -> AllState {
    let total = open(files).count();
    let count = picked(files, checked).len();
    let tri = if total > 0 && count == total {
        Tri::Checked
    } else if count > 0 {
        Tri::Mixed
    } else {
        Tri::Unchecked
    };
    AllState {
        tri,
        off: total == 0,
    }
}

/// A click on "All files": a checked box clears every file; an empty or mixed one checks them
/// all, as a browser checkbox does.
pub fn toggle_all(files: &[ChangeEntry], state: Tri) -> BTreeSet<String> {
    if state == Tri::Checked {
        BTreeSet::new()
    } else {
        first_checks(files)
    }
}

/// A click on one file.
pub fn toggle(checked: &BTreeSet<String>, path: &str, on: bool) -> BTreeSet<String> {
    let mut next = checked.clone();
    if on {
        next.insert(path.to_string());
    } else {
        next.remove(path);
    }
    next
}

/// A conflict is never checked and cannot be.
pub fn row_checked(entry: &ChangeEntry, checked: &BTreeSet<String>) -> bool {
    !is_conflict(entry) && checked.contains(&entry.path)
}

pub fn row_off(entry: &ChangeEntry) -> bool {
    is_conflict(entry)
}

pub fn row_tip(entry: &ChangeEntry) -> Option<&'static str> {
    is_conflict(entry).then_some(CONFLICT_REASON)
}

/// The name of a file's checkbox: "src/app.ts, Modified".
pub fn row_name(entry: &ChangeEntry) -> String {
    format!("{}, {}", entry.path, entry.status.label())
}

/// The last commit does not have these files (a rename: its new name), so a rollback deletes
/// them.
pub fn new_file_count(files: &[ChangeEntry], checked: &BTreeSet<String>) -> usize {
    open(files)
        .filter(|entry| checked.contains(&entry.path))
        .filter(|entry| {
            matches!(
                entry.status,
                ChangeStatus::Added | ChangeStatus::Untracked | ChangeStatus::Renamed
            )
        })
        .count()
}

/// The danger text, or None when no new file is checked.
pub fn new_file_text(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 new file will be deleted.".to_string()),
        _ => Some(format!("{count} new files will be deleted.")),
    }
}

pub fn confirm_label(count: usize) -> String {
    format!("Roll back {}", files_text(count as u64))
}

/// Why the confirm button is off. `picked`: how many files are checked.
pub fn confirm_reason(picked: usize, git_busy: bool) -> Option<&'static str> {
    if picked == 0 {
        Some(NOTHING_CHECKED_REASON)
    } else if git_busy {
        Some(BUSY_REASON)
    } else {
        None
    }
}

/// What a row menu offers to roll back: the files without the conflicts, and why the item is off
/// when none is left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollbackOffer {
    pub entries: Vec<ChangeEntry>,
    pub off_reason: Option<&'static str>,
}

pub fn rollback_offer(entries: Vec<ChangeEntry>) -> RollbackOffer {
    let entries: Vec<ChangeEntry> = entries.into_iter().filter(|e| !is_conflict(e)).collect();
    let off_reason = entries.is_empty().then_some(CONFLICT_REASON);
    RollbackOffer {
        entries,
        off_reason,
    }
}

/// The offered files that are still changed, as the list now has them. A file that went back to
/// its last commit meanwhile is dropped.
pub fn still_changed(offer: &[ChangeEntry], list: &[ChangeEntry]) -> Vec<ChangeEntry> {
    offer
        .iter()
        .filter_map(|asked| list.iter().find(|now| now.path == asked.path))
        .cloned()
        .collect()
}

#[cfg(test)]
#[path = "rollback_rules_tests.rs"]
mod tests;
