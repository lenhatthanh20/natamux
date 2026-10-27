//! Rules of the review comments of the Changes tab that need no editor: how tall a comment block
//! is, where it and its matching empty rows go, which buttons a state shows, and the texts and
//! the grouping of the list. Pure.

use std::ops::Range;

use natamux_proto::workspaces::review_comments::{CommentSide, CommentState, ReviewComment};

use super::change_rules::{Side, lines_label};
use super::comment_rules::Quote;
use super::diff_rules::HunkRows;

/// Body rows of a block; more lines are cut.
pub const MAX_BODY_ROWS: usize = 4;
pub const MORE_TEXT: &str = "\u{2026} more in the list";
/// The lines of an outdated comment's quote that the list shows.
pub const QUOTE_PREVIEW_LINES: usize = 3;
pub const NO_DRAFT_REASON: &str = "No draft comments to send";

/// Where a block goes in an editor, as a 0-based row of that editor's buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    Above(u32),
    Below(u32),
}

impl Spot {
    pub fn row(self) -> u32 {
        match self {
            Spot::Above(row) | Spot::Below(row) => row,
        }
    }
}

/// Where the block of a comment goes: in the left editor or the right one, and the spot of the
/// empty rows of the same height in the other editor (side by side only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub left: bool,
    pub spot: Spot,
    pub other: Option<Spot>,
}

pub fn comment_side(side: Side) -> CommentSide {
    match side {
        Side::Old => CommentSide::Old,
        Side::New => CommentSide::New,
    }
}

/// The lines a block shows, and whether more were cut.
pub fn body_lines(body: &str) -> (Vec<&str>, bool) {
    let lines: Vec<&str> = body.lines().collect();
    let cut = lines.len() > MAX_BODY_ROWS;
    let shown = lines.into_iter().take(MAX_BODY_ROWS).collect();
    (shown, cut)
}

/// The rows of a block: the header, one row per body line, and a row that says lines were cut.
pub fn block_rows(body: &str) -> u32 {
    let (shown, cut) = body_lines(body);
    1 + shown.len().max(1) as u32 + u32::from(cut)
}

/// Resolved comments show only when asked for.
pub fn is_visible(comment: &ReviewComment, show_resolved: bool) -> bool {
    comment.state != CommentState::Resolved || show_resolved
}

/// An outdated comment has no block: its lines are not there any more.
pub fn has_block(comment: &ReviewComment) -> bool {
    !comment.outdated
}

fn whole_file(comment: &ReviewComment) -> bool {
    comment.start == 0 && comment.end == 0
}

fn new_rows(hunk: &HunkRows) -> &Range<u32> {
    &hunk.new
}

fn old_rows(hunk: &HunkRows) -> &Range<u32> {
    &hunk.old
}

/// The row of a side as it is shown beside the other side, which has the empty rows of its
/// shorter hunks. `mine` and `other` pick the rows of each side from a hunk.
fn level_spot(
    row: u32,
    hunks: &[HunkRows],
    mine: fn(&HunkRows) -> &Range<u32>,
    other: fn(&HunkRows) -> &Range<u32>,
) -> Spot {
    let (mut mine_end, mut other_end) = (0, 0);
    for hunk in hunks {
        let (m, o) = (mine(hunk), other(hunk));
        if row < m.start {
            break;
        }
        if row < m.end {
            let index = row - m.start;
            // Past the lines of the other side: the rows of its empty block, so go below them.
            return if index < o.end - o.start {
                Spot::Below(o.start + index)
            } else {
                Spot::Above(o.end)
            };
        }
        mine_end = m.end;
        other_end = o.end;
    }
    Spot::Below((row + other_end).saturating_sub(mine_end))
}

/// Inline: a comment on an old line goes below the last new row before the hunk that holds the
/// line, or above row 1.
pub fn inline_old_spot(row: u32, hunks: &[HunkRows]) -> Spot {
    let (mut old_end, mut new_end) = (0, 0);
    for hunk in hunks {
        if row < hunk.old.start {
            break;
        }
        if row < hunk.old.end {
            return match hunk.new.start.checked_sub(1) {
                Some(above) => Spot::Below(above),
                None => Spot::Above(0),
            };
        }
        old_end = hunk.old.end;
        new_end = hunk.new.end;
    }
    Spot::Below((row + new_end).saturating_sub(old_end))
}

pub fn placement(comment: &ReviewComment, hunks: &[HunkRows], two_sides: bool) -> Placement {
    let left = two_sides && comment.side == CommentSide::Old;
    if whole_file(comment) {
        return Placement {
            left,
            spot: Spot::Above(0),
            other: two_sides.then_some(Spot::Above(0)),
        };
    }
    let row = comment.end.saturating_sub(1);
    if !two_sides {
        let spot = match comment.side {
            CommentSide::New => Spot::Below(row),
            CommentSide::Old => inline_old_spot(row, hunks),
        };
        return Placement {
            left,
            spot,
            other: None,
        };
    }
    let other = match comment.side {
        CommentSide::New => level_spot(row, hunks, new_rows, old_rows),
        CommentSide::Old => level_spot(row, hunks, old_rows, new_rows),
    };
    Placement {
        left,
        spot: Spot::Below(row),
        other: Some(other),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentAction {
    Edit,
    Delete,
    Resolve,
    Reopen,
}

impl CommentAction {
    pub fn label(self) -> &'static str {
        match self {
            CommentAction::Edit => "Edit",
            CommentAction::Delete => "Delete",
            CommentAction::Resolve => "Resolve",
            CommentAction::Reopen => "Reopen",
        }
    }
}

/// The buttons of a block, by the comment's state.
pub fn actions(state: CommentState) -> &'static [CommentAction] {
    match state {
        CommentState::Draft => &[CommentAction::Edit, CommentAction::Delete],
        CommentState::Sent => &[CommentAction::Resolve, CommentAction::Delete],
        CommentState::Resolved => &[CommentAction::Reopen],
    }
}

pub fn state_label(state: CommentState) -> &'static str {
    match state {
        CommentState::Draft => "draft",
        CommentState::Sent => "sent",
        CommentState::Resolved => "resolved",
    }
}

/// The comments of one file, in the order the daemon gave them.
pub struct FileGroup<'a> {
    pub path: &'a str,
    pub comments: Vec<&'a ReviewComment>,
}

/// The list's files in the order of their first comment (the daemon sorts by path).
pub fn group_by_file(comments: &[ReviewComment], show_resolved: bool) -> Vec<FileGroup<'_>> {
    let mut groups: Vec<FileGroup> = Vec::new();
    for comment in comments
        .iter()
        .filter(|comment| is_visible(comment, show_resolved))
    {
        match groups.iter_mut().find(|group| group.path == comment.path) {
            Some(group) => group.comments.push(comment),
            None => groups.push(FileGroup {
                path: &comment.path,
                comments: vec![comment],
            }),
        }
    }
    groups
}

/// Draft and sent comments: the ones still for the agent.
pub fn open_count(comments: &[ReviewComment]) -> usize {
    comments.iter().filter(|comment| comment.open()).count()
}

pub fn draft_count(comments: &[ReviewComment]) -> usize {
    comments
        .iter()
        .filter(|comment| comment.state == CommentState::Draft)
        .count()
}

/// "Send 3 comments"; with none, "Send comments".
pub fn send_label(drafts: usize) -> String {
    match drafts {
        0 => "Send comments".into(),
        1 => "Send 1 comment".into(),
        n => format!("Send {n} comments"),
    }
}

/// "c3 · lines 40–44 · draft".
pub fn row_label(comment: &ReviewComment) -> String {
    let lines = if whole_file(comment) {
        "whole file".to_owned()
    } else {
        lines_label(comment.start, comment.end, "\u{2013}")
    };
    format!(
        "{} \u{b7} {lines} \u{b7} {}",
        comment.id,
        state_label(comment.state)
    )
}

/// The first line of the body, for a list row.
pub fn first_line(body: &str) -> &str {
    body.lines().next().unwrap_or_default()
}

/// The first lines of the quote, for an outdated row.
pub fn quote_preview(comment: &ReviewComment) -> Vec<&str> {
    comment.quote.lines().take(QUOTE_PREVIEW_LINES).collect()
}

/// What Add comment puts the comment on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub side: CommentSide,
    pub start: u32,
    pub end: u32,
}

/// The quote chip's lines and side; else the cursor line of the working tree; else the file.
pub fn target(quote: Option<&Quote>, cursor_line: Option<u32>) -> Target {
    if let Some(quote) = quote {
        return Target {
            side: comment_side(quote.side),
            start: quote.start,
            end: quote.end,
        };
    }
    match cursor_line.filter(|line| *line > 0) {
        Some(line) => Target {
            side: CommentSide::New,
            start: line,
            end: line,
        },
        None => Target {
            side: CommentSide::New,
            start: 0,
            end: 0,
        },
    }
}

#[cfg(test)]
#[path = "review_rules_tests.rs"]
mod tests;
