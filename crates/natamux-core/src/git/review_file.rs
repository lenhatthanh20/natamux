//! What a send of review comments gives the agent: `.natamux-task/review-comments.md` with every
//! open comment, and the short prompt that points to it. Pure.

use natamux_proto::workspaces::review_comments::{
    CommentSide, CommentState, REVIEW_COMMENTS_FILE, ReviewComment,
};

/// A longer quote is left out, as in the comment box: only the line numbers go to the agent.
pub const QUOTE_MAX: usize = 10_000;

pub const OUTDATED_NOTE: &str = "(the code changed; this may be done)";

/// The file: drafts under "New" (the ones this send gives the agent), sent ones under "Still
/// open". Resolved comments are left out, and so is a section with none.
pub fn review_comments_md(comments: &[ReviewComment]) -> String {
    let mut text = String::from("# Review comments\n");
    for (title, state) in [("New", CommentState::Draft), ("Still open", CommentState::Sent)] {
        let items: Vec<&ReviewComment> = comments.iter().filter(|c| c.state == state).collect();
        if items.is_empty() {
            continue;
        }
        text.push_str(&format!("\n## {title}\n"));
        for comment in items {
            text.push('\n');
            text.push_str(&item(comment));
        }
    }
    text
}

fn item(comment: &ReviewComment) -> String {
    let old = comment.side == CommentSide::Old;
    let mut text = format!("### {}: {}, {}", comment.id, comment.path, where_(comment));
    if old && comment.start > 0 {
        text.push_str(" (before the change)");
    }
    text.push('\n');
    if comment.outdated {
        text.push_str(OUTDATED_NOTE);
        text.push('\n');
    }
    if comment.start > 0 {
        if comment.quote.encode_utf16().count() > QUOTE_MAX {
            let source = if old { "the last commit" } else { "the file" };
            text.push_str(&format!("(Too long to quote. Read these lines in {source}.)\n"));
        } else {
            text.push_str(&fenced(&comment.quote));
            text.push('\n');
        }
    }
    text.push_str(comment.body.trim());
    text.push('\n');
    text
}

fn where_(comment: &ReviewComment) -> String {
    match (comment.start, comment.end) {
        (0, _) => "whole file".into(),
        (start, end) if start == end => format!("line {start}"),
        (start, end) => format!("lines {start}-{end}"),
    }
}

/// A fence longer than any run of backticks in the code, so the code cannot end the block (as
/// `fenced` in the app's `comment_rules.rs`).
fn fenced(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}\n{text}\n{fence}")
}

/// The prompt pasted into the agent's terminal.
pub fn send_prompt(new: usize) -> String {
    format!(
        "Review comments: {new} new. Read .natamux-task/{REVIEW_COMMENTS_FILE}. Fix each item \
         under New, then reply with what you changed per id (c3: …)."
    )
}

#[cfg(test)]
#[path = "review_file_tests.rs"]
mod tests;
