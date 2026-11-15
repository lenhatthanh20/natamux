//! The rules of the code view that need no window (plan 6.4): the indent guides and the active
//! guide, the typing facts of a language with what they decide (auto-close, Enter between
//! brackets, the indent of a new line, strings and comments), and the search for the other places
//! of the selected text.
//! The rules give what the old editor gave: the outline and code typing golden files of the old
//! code hold the expected results.

use std::ops::Range;

use tree_sitter::{Node, Tree};

use super::text_input::TAB_SIZE;

/// One indent guide: the rows it runs over and its depth (0 is the first indent level).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Guide {
    pub start: usize,
    pub end: usize,
    pub depth: usize,
}

fn blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// The columns of a line's indent; a tab goes to the next tab stop.
fn columns(line: &str) -> usize {
    let mut column = 0;
    for c in line.chars() {
        match c {
            ' ' => column += 1,
            '\t' => column += TAB_SIZE - column % TAB_SIZE,
            _ => break,
        }
    }
    column
}

/// The leading spaces and tabs of a line.
fn lead(line: &str) -> &str {
    let end = line.len() - line.trim_start_matches([' ', '\t']).len();
    &line[..end]
}

/// The indent guides of `text`, inner ones first. A line has one level per whole tab stop of its
/// indent; a blank line takes the level of the next line that is not blank, so a guide never ends
/// on the blank lines after its block.
pub fn guides(text: &str) -> Vec<Guide> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut levels = vec![0; lines.len()];
    let mut next = 0;
    for (row, line) in lines.iter().enumerate().rev() {
        if !blank(line) {
            next = columns(line) / TAB_SIZE;
        }
        levels[row] = next;
    }
    let mut out = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut last = 0;
    for (row, &level) in levels.iter().enumerate() {
        while open.len() > level {
            if let Some(start) = open.pop() {
                out.push(Guide {
                    start,
                    end: last,
                    depth: open.len(),
                });
            }
        }
        while open.len() < level {
            open.push(row);
        }
        last = row;
    }
    while let Some(start) = open.pop() {
        out.push(Guide {
            start,
            end: last,
            depth: open.len(),
        });
    }
    out
}

/// The guides drawn as active with the caret on `row`, as indices of `guides`. The caret's block
/// is the run of rows around it indented at least as far as its row (or as the row below, when
/// that row starts a deeper block); the active guides are the ones inside that block at the
/// indent of the line above it.
pub fn active_guides(text: &str, guides: &[Guide], row: usize) -> Vec<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let none = Vec::new();
    if row >= lines.len() {
        return none;
    }
    let is_blank = |at: usize| blank(lines[at]);
    let previous = |at: usize| (0..at).rev().find(|&row| !is_blank(row));
    let next = |at: usize| (at + 1..lines.len()).find(|&row| !is_blank(row));
    let opens = !is_blank(row)
        && row + 1 < lines.len()
        && !is_blank(row + 1)
        && columns(lines[row + 1]) > columns(lines[row]);
    let (target, from) = if opens {
        if row == 0 {
            return none;
        }
        (columns(lines[row + 1]), row + 1)
    } else if is_blank(row) {
        match (previous(row), next(row)) {
            (Some(above), Some(below)) => (columns(lines[above]).max(columns(lines[below])), row),
            _ => (0, row),
        }
    } else {
        (columns(lines[row]), row)
    };
    if target == 0 {
        return none;
    }
    let inside = |at: usize| is_blank(at) || columns(lines[at]) >= target;
    let mut start = from;
    while start > 0 && inside(start - 1) {
        start -= 1;
    }
    let mut end = from;
    while end + 1 < lines.len() && inside(end + 1) {
        end += 1;
    }
    while end > start && is_blank(end) {
        end -= 1;
    }
    let Some(opener) = previous(start).filter(|_| start > 0) else {
        return none;
    };
    let indent = columns(lines[opener]);
    guides
        .iter()
        .enumerate()
        .filter(|(_, guide)| {
            guide.depth * TAB_SIZE == indent && guide.start >= start && guide.end <= end
        })
        .map(|(index, _)| index)
        .collect()
}

/// The pair closes itself when its opening is typed.
pub const AUTO: u8 = 1;
/// Enter between the two puts the caret on a line of its own and the closing part on the next.
pub const NEWLINE: u8 = 2;
/// The pair does not close itself inside a string.
pub const NO_STRING: u8 = 4;
/// The pair does not close itself inside a comment.
pub const NO_COMMENT: u8 = 8;

/// A bracket or quote pair of a language. The opening may be more than one char (`/*`, `f"`):
/// it counts once its last char is typed.
pub struct Pair {
    pub open: &'static str,
    pub close: &'static str,
    pub auto: bool,
    pub newline: bool,
    pub not_in_string: bool,
    pub not_in_comment: bool,
}

/// A pair with the flags above.
pub const fn pair(open: &'static str, close: &'static str, flags: u8) -> Pair {
    Pair {
        open,
        close,
        auto: flags & AUTO != 0,
        newline: flags & NEWLINE != 0,
        not_in_string: flags & NO_STRING != 0,
        not_in_comment: flags & NO_COMMENT != 0,
    }
}

/// Whether a place is inside a string or a comment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scope {
    pub string: bool,
    pub comment: bool,
}

/// The typing facts of a language (plan 6.4), as the old editor applied them. The values of
/// each language are in `code_langs.rs`.
pub struct Typing {
    pub pairs: &'static [Pair],
    /// The chars after the caret that still let a pair close itself; a space, a line end and the
    /// end of the text always do.
    pub close_before: &'static str,
    /// Chars that count as word chars for word moves, next to letters, digits and `_`.
    pub word_chars: &'static str,
    /// The line comment starts. Enter on a line that starts with one goes on with it.
    pub line_comments: &'static [&'static str],
    /// The opening of a doc comment (`/**` or `/*`). Enter in one goes on with ` * `.
    pub doc_comment: Option<&'static str>,
    /// A tab char instead of spaces for each indent level.
    pub hard_tabs: bool,
    /// Markdown lists go on at Enter (`code_enter.rs`).
    pub lists: bool,
    /// Typing the `>` of a JSX opening tag adds its closing tag.
    pub jsx: bool,
    /// The node kinds whose inside is one level deeper, each with the token that ends it (empty:
    /// the node's own end).
    pub indent_nodes: &'static [(&'static str, &'static str)],
    /// Any node whose children hold one of these opening tokens and its closing token is such a
    /// block too, ended by the closing token.
    pub indent_brackets: &'static [(&'static str, &'static str)],
    /// True for a line that makes the next line one level deeper (`code_langs.rs`).
    pub indent_after: Option<fn(&str) -> bool>,
    /// The node kinds that are strings and comments, for `Pair::not_in_string` and
    /// `not_in_comment`.
    pub string_nodes: &'static [&'static str],
    pub comment_nodes: &'static [&'static str],
}

impl Typing {
    fn is_word(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || self.word_chars.contains(c)
    }

    /// One indent level.
    pub fn indent_unit(&self) -> String {
        if self.hard_tabs {
            "\t".to_owned()
        } else {
            " ".repeat(TAB_SIZE)
        }
    }

    /// What closes the pair whose opening ends with `c` typed at `at`, when it closes itself
    /// there: a pair that closes itself, not in a string or comment it stays out of, only before
    /// a space, a line end, the end or a char of `close_before`, and a quote not right after a
    /// word char. The longest opening that fits wins.
    pub fn closer(&self, text: &str, at: usize, c: char, scope: Scope) -> Option<&'static str> {
        let before = &text[..at];
        let mut buffer = [0; 4];
        let typed: &str = c.encode_utf8(&mut buffer);
        let pair = self
            .pairs
            .iter()
            .filter(|pair| {
                pair.open.ends_with(typed)
                    && before.ends_with(&pair.open[..pair.open.len() - typed.len()])
            })
            .max_by_key(|pair| pair.open.len())?;
        if !pair.auto
            || (pair.not_in_string && scope.string)
            || (pair.not_in_comment && scope.comment)
        {
            return None;
        }
        let next = text[at..].chars().next();
        if !next.is_none_or(|next| next.is_whitespace() || self.close_before.contains(next)) {
            return None;
        }
        let after_word = before.chars().next_back().is_some_and(|c| self.is_word(c));
        if pair.open == pair.close && after_word {
            return None;
        }
        Some(pair.close)
    }

    /// The pair that surrounds a selection when its one-char opening is typed over it.
    pub fn surround(&self, c: char) -> Option<&'static str> {
        let mut buffer = [0; 4];
        let typed: &str = c.encode_utf8(&mut buffer);
        self.pairs
            .iter()
            .find(|pair| pair.open == typed)
            .map(|pair| pair.close)
    }

    /// Whether the caret at `at` sits between the two parts of a pair that opens a line of its
    /// own on Enter.
    pub fn expands(&self, text: &str, at: usize) -> bool {
        let (before, after) = text.split_at(at);
        self.pairs
            .iter()
            .any(|pair| pair.newline && before.ends_with(pair.open) && after.starts_with(pair.close))
    }

    /// The line comment start that `before` (a line up to the caret) begins with, after its
    /// indent: the longest one.
    pub fn line_comment(&self, before: &str) -> Option<&'static str> {
        let text = before.trim_start_matches([' ', '\t']);
        self.line_comments
            .iter()
            .filter(|start| text.starts_with(start.trim_end()))
            .max_by_key(|start| start.trim_end().len())
            .copied()
    }

    /// The indent of `row` of `text` (parsed as `tree`), from the row above that is not blank: one
    /// level more when a block (`indent_nodes`, `indent_brackets`) starts on that row and goes on
    /// past this row's start, or when `indent_after` holds for that row; the indent of a block's
    /// first row when the block ended between the two rows; else the same.
    pub fn indent_for(&self, text: &str, tree: &Tree, row: usize) -> String {
        let lines: Vec<&str> = text.split('\n').collect();
        let Some(above) = (0..row.min(lines.len()))
            .rev()
            .find(|&at| !blank(lines[at]))
        else {
            return String::new();
        };
        let start_of = |at: usize| -> usize { lines[..at].iter().map(|line| line.len() + 1).sum() };
        let (row_start, above_start) = (start_of(row), start_of(above));
        let blocks = self.blocks(tree, row);
        let deeper = blocks
            .iter()
            .any(|&(first, end)| first == above && end > row_start)
            || self.indent_after.is_some_and(|opens| opens(lines[above]));
        if deeper {
            return format!("{}{}", lead(lines[above]), self.indent_unit());
        }
        let ended = blocks
            .iter()
            .filter(|&&(_, end)| end > above_start && end <= row_start)
            .map(|&(first, _)| first)
            .min();
        match ended {
            Some(first) if first < above => lead(lines[first]).to_owned(),
            _ => lead(lines[above]).to_owned(),
        }
    }

    /// The blocks that start before `row`: the first row of each and the byte its end token
    /// starts at (the node's end when it has none).
    fn blocks(&self, tree: &Tree, row: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut nodes = vec![tree.root_node()];
        let mut cursor = tree.walk();
        while let Some(node) = nodes.pop() {
            if node.start_position().row >= row {
                continue;
            }
            let children: Vec<Node> = node.children(&mut cursor).collect();
            let end_of = |end: &str| {
                children
                    .iter()
                    .rev()
                    .find(|child| child.kind() == end && !child.is_missing())
                    .map_or(node.end_byte(), |child| child.start_byte())
            };
            let kind = self
                .indent_nodes
                .iter()
                .find(|(kind, _)| *kind == node.kind())
                .map(|(_, end)| *end);
            let bracket = self.indent_brackets.iter().find(|(open, close)| {
                children
                    .iter()
                    .any(|child| child.kind() == *open && !child.is_missing())
                    && children.iter().any(|child| child.kind() == *close)
            });
            if let Some(end) = kind.or(bracket.map(|(_, close)| *close)) {
                out.push((node.start_position().row, end_of(end)));
            }
            nodes.extend(children);
        }
        out
    }

    /// Whether `at` is inside a string or a comment node of `tree`: not at an edge, except the
    /// end of a comment that runs to the line end.
    pub fn scope_at(&self, tree: &Tree, text: &str, at: usize) -> Scope {
        let mut scope = Scope::default();
        let mut node = tree
            .root_node()
            .descendant_for_byte_range(at.saturating_sub(1), at);
        while let Some(found) = node {
            node = found.parent();
            let (string, comment) = (
                self.string_nodes.contains(&found.kind()),
                self.comment_nodes.contains(&found.kind()),
            );
            let open_end = comment
                && !text
                    .get(found.byte_range())
                    .is_some_and(|text| text.ends_with("*/"));
            let inside = found.start_byte() < at
                && (at < found.end_byte() || (at == found.end_byte() && open_end));
            scope.string |= inside && string;
            scope.comment |= inside && comment;
        }
        scope
    }

    /// Whether `at` is inside a doc comment: a comment node that starts with `doc_comment`.
    pub fn in_doc_comment(&self, tree: &Tree, text: &str, at: usize) -> bool {
        let Some(start) = self.doc_comment else {
            return false;
        };
        let mut node = tree
            .root_node()
            .descendant_for_byte_range(at.saturating_sub(1), at);
        while let Some(found) = node {
            node = found.parent();
            if self.comment_nodes.contains(&found.kind())
                && found.start_byte() < at
                && at < found.end_byte()
                && text
                    .get(found.byte_range())
                    .is_some_and(|text| text.starts_with(start))
            {
                return true;
            }
        }
        false
    }
}

/// The places of `query` in `text`, without overlaps. The case does not count, as in Zed's
/// search for the selected text: only ASCII letters for an ASCII query, else by lower case.
pub fn find_all(text: &str, query: &str) -> Vec<Range<usize>> {
    let ascii = query.is_ascii();
    let same = |a: char, b: char| {
        if ascii {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b || a.to_lowercase().eq(b.to_lowercase())
        }
    };
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(first) = text[from..].chars().next() {
        let mut rest = text[from..].char_indices();
        let mut end = Some(from);
        for wanted in query.chars() {
            end = match rest.next() {
                Some((at, c)) if same(c, wanted) => Some(from + at + c.len_utf8()),
                _ => None,
            };
            if end.is_none() {
                break;
            }
        }
        match end {
            Some(end) if end > from => {
                found.push(from..end);
                from = end;
            }
            _ => from += first.len_utf8(),
        }
    }
    found
}

#[cfg(test)]
#[path = "code_rules_tests.rs"]
mod tests;
