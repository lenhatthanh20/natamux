//! The review comment for the agent (`features/git/AgentComment.tsx`): the text the agent gets,
//! and the label, placeholder and chip texts of the comment box. Pure.

// The comment box uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

pub use crate::git::change_rules::Side;
use crate::git::change_rules::lines_label;

/// The message route takes 20,000 characters. A longer quote would leave no room for the comment.
pub const QUOTE_MAX: usize = 10_000;

pub const SEND_REASON: &str = "Write a comment first";
pub const SENT_TEXT: &str = "Sent to the agent.";
pub const REMOVE_QUOTE_LABEL: &str = "Remove the selected lines";
pub const PLACEHOLDER: &str = "What should change, and why";
pub const QUOTE_PLACEHOLDER: &str = "Ask about these lines, or say what should change";
const TOO_LONG_NOTE: &str = " · too long to quote, only the line numbers are sent";

/// Lines picked in the diff, to quote to the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    pub side: Side,
    pub start: u32,
    pub end: u32,
    pub text: String,
}

impl Quote {
    /// Longer than `QUOTE_MAX`, counted in UTF-16 units like the web client, so only the line
    /// numbers go to the agent.
    pub fn too_long(&self) -> bool {
        self.text.encode_utf16().count() > QUOTE_MAX
    }
}

/// A fence longer than any run of backticks in the code, so the code cannot end the block.
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

/// The text the agent gets: the file (and line) first, so the agent knows where to look.
pub fn comment_for_agent(
    path: &str,
    comment: &str,
    line: Option<u32>,
    quote: Option<&Quote>,
) -> String {
    let comment = comment.trim();
    let Some(quote) = quote else {
        let where_ = match line.filter(|line| *line > 0) {
            Some(line) => format!("{path}, line {line}"),
            None => path.to_string(),
        };
        return format!("Review comment on {where_}:\n{comment}");
    };
    let old = quote.side == Side::Old;
    let code = if quote.too_long() {
        let source = if old { "the last commit" } else { "the file" };
        format!("(Too long to quote. Read these lines in {source}.)")
    } else {
        fenced(&quote.text)
    };
    let lines = lines_label(quote.start, quote.end, "-");
    let before = if old { " before the change" } else { "" };
    format!("Review comment on {path}, {lines}{before}:\n{code}\n{comment}")
}

/// "lines 12–14 (before the change)", for the label and the chip.
fn quote_label(quote: &Quote) -> String {
    let before = if quote.side == Side::Old {
        " (before the change)"
    } else {
        ""
    };
    format!("{}{before}", lines_label(quote.start, quote.end, "–"))
}

/// The label of the field.
pub fn comment_label(line: Option<u32>, quote: Option<&Quote>) -> String {
    if let Some(quote) = quote {
        return format!("Comment on {} for the agent", quote_label(quote));
    }
    match line.filter(|line| *line > 0) {
        Some(line) => format!("Comment on line {line} for the agent"),
        None => "Comment on this file for the agent".to_string(),
    }
}

pub fn placeholder(quote: Option<&Quote>) -> &'static str {
    if quote.is_some() {
        QUOTE_PLACEHOLDER
    } else {
        PLACEHOLDER
    }
}

/// The text of the chip over the field: "Lines 12–14", with a note for a quote that is too long.
pub fn chip_text(quote: &Quote) -> String {
    let label = quote_label(quote);
    // The label starts with a lower case "l"; the chip is a title.
    let mut chip = label.replacen('l', "L", 1);
    if quote.too_long() {
        chip.push_str(TOO_LONG_NOTE);
    }
    chip
}

/// Why Send is off: nothing but white space was written.
pub fn send_reason(comment: &str) -> Option<&'static str> {
    comment.trim().is_empty().then_some(SEND_REASON)
}

#[cfg(test)]
#[path = "comment_rules_tests.rs"]
mod tests;
