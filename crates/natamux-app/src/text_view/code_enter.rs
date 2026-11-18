//! What Enter does in a code view with typing facts (plan 6.4), in this order:
//! - a Markdown list item goes on with the next marker; an empty item loses its indent level, or
//!   its marker at the first level;
//! - a line comment goes on with its start;
//! - a doc comment goes on with ` * `, and its end moves to a line of its own;
//! - between the two parts of a pair that opens a block (spaces between do not count), the caret
//!   goes on a line of its own and the closing part on the next;
//! - after the one statement of a body with no braces (`if (a)` + `b`), the new line goes back
//!   to the indent of the `if`;
//! - else the new line takes the indent the language gives it; the spaces after the caret stay
//!   as part of it when they fit, else they go.

use std::ops::Range;

use tree_sitter::Tree;

use super::code_rules::Typing;
use super::text_input::TAB_SIZE;
use crate::git::languages::Lang;
use crate::text::motion;

/// One edit of Enter: what replaces `range`, and where the caret goes.
#[derive(Debug, PartialEq, Eq)]
pub struct Enter {
    pub range: Range<usize>,
    pub text: String,
    pub caret: usize,
}

/// `text` parsed in `lang`.
pub(super) fn parse(lang: &Lang, text: &str) -> Option<Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).ok()?;
    parser.parse(text, None)
}

/// The leading spaces and tabs of `line`.
fn lead_of(line: &str) -> &str {
    let end = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..end]
}

/// The row of the byte `at` of `text`.
fn row_of(text: &str, at: usize) -> usize {
    text[..at].matches('\n').count()
}

/// The edit Enter makes over `range` of `text`. `tree` is the parse of `text`, when there is one.
pub fn enter(
    typing: &Typing,
    lang: &Lang,
    text: &str,
    range: Range<usize>,
    tree: Option<&Tree>,
) -> Enter {
    let line_start = motion::line_start(text, range.start);
    let line_end = motion::line_end(text, range.start);
    let lead = lead_of(&text[line_start..line_end]).to_owned();
    let before = &text[line_start..range.start];
    let insert = |inserted: String| Enter {
        caret: range.start + inserted.len(),
        range: range.clone(),
        text: inserted,
    };
    if typing.lists
        && range.is_empty()
        && let Some(edit) = list_item(text, line_start..line_end, range.start)
    {
        return edit;
    }
    if let Some(start) = typing.line_comment(before) {
        return insert(format!("\n{lead}{start}"));
    }
    if let Some(edit) = doc_comment(typing, text, &range, tree, &lead) {
        return edit;
    }
    let scope = tree.map_or_else(Default::default, |tree| {
        typing.scope_at(tree, text, range.start)
    });
    if range.is_empty() && typing.expands(text, range.start, scope) {
        // The spaces after the caret go; the ones before it stay on the line.
        let at = range.start;
        let rest = &text[at..];
        let end = at + rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let after = format!("{}\n\n{lead}{}", &text[..at], &text[end..]);
        let middle = indent(typing, lang, &after, row_of(text, at) + 1, &lead);
        let inserted = format!("\n{middle}\n{lead}");
        return Enter {
            range: at..end,
            caret: at + 1 + middle.len(),
            text: inserted,
        };
    }
    let rest = &text[range.end..];
    let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let end = range.end + spaces;
    let after = format!("{}\n{}", &text[..range.start], &text[end..]);
    let new_indent = match tree.and_then(|tree| after_braceless(text, tree, range.start)) {
        Some(lead) => lead,
        None => indent(typing, lang, &after, row_of(text, range.start) + 1, &lead),
    };
    // Spaces after the caret that fit in the new indent stay, as part of it.
    let kept = &rest[..spaces];
    if !kept.is_empty()
        && kept.len() <= new_indent.len()
        && kept
            .bytes()
            .chain(new_indent.bytes())
            .all(|byte| byte == b' ')
    {
        let inserted = format!("\n{}", &new_indent[kept.len()..]);
        return Enter {
            caret: range.start + inserted.len(),
            range,
            text: inserted,
        };
    }
    let inserted = format!("\n{new_indent}");
    Enter {
        range: range.start..end,
        caret: range.start + inserted.len(),
        text: inserted,
    }
}

/// The statements that may hold one statement with no braces as their body.
const BRACELESS: [&str; 6] = [
    "if_statement",
    "else_clause",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];
/// The bodies in braces, which keep the next line inside.
const BLOCKS: [&str; 3] = ["compound_statement", "statement_block", "block"];

/// When `at` ends the one statement of a body with no braces (`if (a)` then `b;` on the line
/// below), the indent of the line that holds its `if`: the line after the body goes back there.
fn after_braceless(text: &str, tree: &Tree, at: usize) -> Option<String> {
    let from = at.checked_sub(1)?;
    let mut node = tree.root_node().descendant_for_byte_range(from, from)?;
    while let Some(parent) = node.parent() {
        if BRACELESS.contains(&parent.kind()) {
            let mut cursor = parent.walk();
            let body = parent
                .children(&mut cursor)
                .filter(|child| child.end_byte() > child.start_byte())
                .last()?;
            let statement = body.kind().ends_with("_statement") && !BLOCKS.contains(&body.kind());
            if statement && body.end_byte() == at {
                let start = motion::line_start(text, parent.start_byte());
                return Some(lead_of(&text[start..]).to_owned());
            }
        }
        node = parent;
    }
    None
}

/// The indent of `row` of `text` by the language's rules, `lead` when it does not parse.
fn indent(typing: &Typing, lang: &Lang, text: &str, row: usize, lead: &str) -> String {
    parse(lang, text).map_or_else(
        || lead.to_owned(),
        |tree| typing.indent_for(text, &tree, row),
    )
}

/// Enter in a doc comment: on its first line, the next line starts with ` * ` and an end right
/// after the caret moves to the line after that; on a middle line (` * ...`), the next line
/// starts the same way.
fn doc_comment(
    typing: &Typing,
    text: &str,
    range: &Range<usize>,
    tree: Option<&Tree>,
    lead: &str,
) -> Option<Enter> {
    let start = typing.doc_comment?;
    let line_start = motion::line_start(text, range.start);
    let before = text[line_start..range.start].trim_start_matches([' ', '\t']);
    if before.starts_with(start) && !before[start.len()..].contains("*/") {
        let middle = format!("\n{lead} * ");
        let rest = &text[range.end..];
        let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let (end, inserted) = if rest[spaces..].starts_with("*/") {
            (range.end + spaces, format!("{middle}\n{lead} "))
        } else {
            (range.end, middle.clone())
        };
        return Some(Enter {
            range: range.start..end,
            caret: range.start + middle.len(),
            text: inserted,
        });
    }
    let middle_line = before.starts_with('*') && !before.starts_with("*/");
    if middle_line && tree.is_some_and(|tree| typing.in_doc_comment(tree, text, range.start)) {
        let inserted = format!("\n{lead}* ");
        return Some(Enter {
            range: range.clone(),
            caret: range.start + inserted.len(),
            text: inserted,
        });
    }
    None
}

/// The marker of a Markdown list item at the start of `item` (the line after its indent), and
/// the marker of the next item: a task goes on as an open task, a number counts up.
fn list_marker(item: &str) -> Option<(usize, String)> {
    for task in ["- [ ] ", "- [x] ", "- [X] "] {
        if item.starts_with(task) {
            return Some((task.len(), "- [ ] ".to_owned()));
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if item.starts_with(bullet) {
            return Some((bullet.len(), bullet.to_owned()));
        }
    }
    let digits = item.len() - item.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 && item[digits..].starts_with(". ") {
        let number: u64 = item[..digits].parse().ok()?;
        return Some((digits + 2, format!("{}. ", number.saturating_add(1))));
    }
    None
}

/// Enter at `at` on a Markdown list item `line`: the next item gets the next marker at the same
/// indent; an empty item goes back one indent level, or loses its marker at the first level.
fn list_item(text: &str, line: Range<usize>, at: usize) -> Option<Enter> {
    let lead = lead_of(&text[line.clone()]);
    let item_start = line.start + lead.len();
    let (marker, next) = list_marker(&text[item_start..line.end])?;
    if at < item_start + marker {
        return None;
    }
    if text[item_start + marker..line.end].trim().is_empty() {
        if lead.is_empty() {
            return Some(Enter {
                range: line.start..line.end,
                text: String::new(),
                caret: line.start,
            });
        }
        let back = lead.len().min(TAB_SIZE);
        return Some(Enter {
            range: line.start..line.start + back,
            text: String::new(),
            caret: at - back,
        });
    }
    let inserted = format!("\n{lead}{next}");
    Some(Enter {
        range: at..at,
        caret: at + inserted.len(),
        text: inserted,
    })
}

#[cfg(test)]
#[path = "code_enter_tests.rs"]
mod tests;
