//! Rules of the Changes tab that need no window: paths, line counts, the change navigator, file
//! stepping, what the body of a file shows (`lib/changes.ts`, `features/git/body-kind.ts`,
//! `FileBody.tsx`), and the colour of a status.

// The Changes tab uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::workspaces::changes::{ChangeStatus, FileDiff, image_type_of};

use crate::git::change_tree::LineCounts;

/// Which way a step goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Next,
    Prev,
}

/// The folder (with its last `/`) and the name, the way VS Code lists a change.
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(slash) => path.split_at(slash + 1),
        None => ("", path),
    }
}

pub fn count_label(count: usize) -> String {
    match count {
        1 => "1 changed file".to_string(),
        _ => format!("{count} changed files"),
    }
}

/// A file that is new or gone is not a diff. It is shown whole, as all added or all removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeKind {
    Added,
    Removed,
}

pub fn whole_file_kind(status: ChangeStatus) -> Option<WholeKind> {
    match status {
        ChangeStatus::Added | ChangeStatus::Untracked => Some(WholeKind::Added),
        ChangeStatus::Deleted => Some(WholeKind::Removed),
        _ => None,
    }
}

/// Drops one final line break, so an editor shows no empty last line.
pub fn trim_trailing_newline(text: &str) -> &str {
    match text.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest),
        None => text,
    }
}

pub fn count_lines(text: &str) -> usize {
    let trimmed = trim_trailing_newline(text);
    if trimmed.is_empty() {
        0
    } else {
        trimmed.split('\n').count()
    }
}

/// A change of the diff, in lines of both sides, 1-based. An end line of 0 means the change has
/// no lines on that side. Built from the hunks of the buffer diff.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineChange {
    pub original_start: u32,
    pub original_end: u32,
    pub modified_start: u32,
    pub modified_end: u32,
}

pub fn count_line_changes(changes: &[LineChange]) -> LineCounts {
    let span = |start: u32, end: u32| -> u64 {
        if end > 0 {
            u64::from(end.saturating_sub(start)) + 1
        } else {
            0
        }
    };
    changes.iter().fold(
        LineCounts {
            added: 0,
            removed: 0,
        },
        |sum, change| LineCounts {
            added: sum.added + span(change.modified_start, change.modified_end),
            removed: sum.removed + span(change.original_start, change.original_end),
        },
    )
}

/// The lines a change covers on the new side. A change with no new lines (a deletion) sits at
/// the line it follows, so the cursor can still go there.
pub fn change_anchor(change: &LineChange) -> (u32, u32) {
    let start = change.modified_start.max(1);
    (start, change.modified_end.max(start))
}

/// The change that holds `line` of the new side.
pub fn change_at(changes: &[LineChange], line: u32) -> Option<usize> {
    changes.iter().position(|change| {
        let (start, end) = change_anchor(change);
        start <= line && line <= end
    })
}

/// The change to go to from `line`. Unlike files, changes wrap at both ends, as in VS Code.
/// Without a line yet, the first step goes to the first or the last change. None when there is
/// no change.
pub fn step_change(changes: &[LineChange], line: Option<u32>, by: Step) -> Option<usize> {
    let last = changes.len().checked_sub(1)?;
    let Some(line) = line else {
        return Some(if by == Step::Next { 0 } else { last });
    };
    let starts: Vec<u32> = changes.iter().map(|c| change_anchor(c).0).collect();
    Some(match by {
        Step::Next => starts.iter().position(|start| *start > line).unwrap_or(0),
        Step::Prev => starts
            .iter()
            .rposition(|start| *start < line)
            .unwrap_or(last),
    })
}

/// "2 of 7" when the cursor is in a change, else the count.
pub fn change_count_label(index: Option<usize>, total: usize) -> String {
    match (index, total) {
        (Some(index), _) => format!("{} of {total}", index + 1),
        (None, 1) => "1 change".to_string(),
        (None, _) => format!("{total} changes"),
    }
}

/// The file before or after the current one. It stops at the ends, it does not wrap.
pub fn step_file<'a>(paths: &'a [String], current: Option<&str>, step: Step) -> Option<&'a str> {
    let last = paths.len().checked_sub(1)?;
    let index = current
        .and_then(|path| paths.iter().position(|p| p == path))
        .map_or(-1, |index| index as isize);
    let by = if step == Step::Next { 1 } else { -1 };
    let next = (index + by).clamp(0, last as isize) as usize;
    paths.get(next).map(String::as_str)
}

/// Prose has long lines that are better wrapped. Code is not.
pub fn wraps_by_default(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        None | Some(0) => true,
        Some(dot) => {
            let ext = name[dot + 1..].to_ascii_lowercase();
            matches!(ext.as_str(), "md" | "markdown" | "txt" | "rst")
        }
    }
}

/// Which side of the diff a picked line is on. `Old` lines are from the last commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    New,
    Old,
}

/// The lines a selection touches. A drag over full lines ends at column 1 of the next line,
/// which the user did not mean to pick. Columns are 1-based.
pub fn whole_lines(start_line: u32, end_line: u32, end_column: u32) -> (u32, u32) {
    if end_column == 1 && end_line > start_line {
        (start_line, end_line - 1)
    } else {
        (start_line, end_line)
    }
}

/// "line 12" or "lines 12–14". The agent gets a plain hyphen, the UI an en dash.
pub fn lines_label(start: u32, end: u32, dash: &str) -> String {
    if start == end {
        format!("line {start}")
    } else {
        format!("lines {start}{dash}{end}")
    }
}

/// What the body of a file shows. The toolbar reads it, so it has no buttons that do nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Text,
    Whole,
    Image,
    Notice,
    /// The file is not read yet.
    Pending,
}

/// An image that cannot be shown as text, or an SVG that the reader wants to see drawn.
pub fn shows_image(file: &FileDiff, svg_preview: bool) -> bool {
    image_type_of(&file.path).is_some() && (file.binary || file.too_large || svg_preview)
}

/// The text of a new or gone file; empty when its side is missing.
pub fn whole_text(file: &FileDiff, kind: WholeKind) -> &str {
    let side = match kind {
        WholeKind::Added => &file.modified,
        WholeKind::Removed => &file.original,
    };
    side.as_deref().unwrap_or("")
}

/// The order of checks is the order `FileBody` follows.
pub fn body_kind(file: Option<&FileDiff>, svg_preview: bool, same_text: bool) -> BodyKind {
    let Some(file) = file else {
        return BodyKind::Pending;
    };
    if shows_image(file, svg_preview) {
        return BodyKind::Image;
    }
    if file.binary || file.too_large {
        return BodyKind::Notice;
    }
    if let Some(kind) = whole_file_kind(file.status) {
        return if whole_text(file, kind).is_empty() {
            BodyKind::Notice
        } else {
            BodyKind::Whole
        };
    }
    if same_text {
        BodyKind::Notice
    } else {
        BodyKind::Text
    }
}

/// The sentence of a file whose `body_kind` is `Notice`. Same order as `body_kind`.
pub fn notice_text(file: &FileDiff, same_text: bool) -> Option<&'static str> {
    if file.binary {
        return Some("This file is binary. It changed, but it is not shown.");
    }
    if file.too_large {
        return Some("This file is larger than 1 MB, so it is not shown.");
    }
    if let Some(kind) = whole_file_kind(file.status) {
        return whole_text(file, kind).is_empty().then_some("Empty file.");
    }
    if !same_text {
        return None;
    }
    Some(if file.status == ChangeStatus::Renamed {
        "Renamed without changes."
    } else {
        "The text did not change. Only the file mode may differ."
    })
}

/// The colour family of a status letter. The colour lookup is in `components/colors.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusTone {
    Modified,
    Created,
    Deleted,
    Renamed,
}

pub fn status_tone(status: ChangeStatus) -> StatusTone {
    match status {
        ChangeStatus::Modified => StatusTone::Modified,
        ChangeStatus::Added | ChangeStatus::Untracked => StatusTone::Created,
        ChangeStatus::Deleted | ChangeStatus::Conflict => StatusTone::Deleted,
        ChangeStatus::Renamed => StatusTone::Renamed,
    }
}

#[cfg(test)]
#[path = "change_rules_tests.rs"]
mod tests;
