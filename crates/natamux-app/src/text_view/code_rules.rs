//! The rules of the code view that need no window (plan 6.4): the indent guides and the active
//! guide, and the typing facts of a language with what they decide (auto-close, Enter between
//! brackets, the indent of a new line). The rules give what the old editor gave: the outline and
//! code typing golden files of the old code hold the expected results.

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

/// A bracket or quote pair of a language. `newline`: Enter between the two puts the caret on an
/// indented line of its own and the closing char on the line after.
pub struct Pair {
    pub open: char,
    pub close: char,
    pub newline: bool,
}

/// The typing facts of a language (plan 6.4), as the old editor applied them.
pub struct Typing {
    pub pairs: &'static [Pair],
    /// The chars after the caret that still let a pair close itself; a space, a line end and the
    /// end of the text always do.
    pub close_before: &'static str,
    /// Chars that count as word chars for word moves, next to letters, digits and `_`.
    pub word_chars: &'static str,
    /// The node kinds whose inside is one level deeper, each with the token that ends it.
    pub indent_nodes: &'static [(&'static str, &'static str)],
    /// Inside these nodes no pair closes itself.
    pub string_nodes: &'static [&'static str],
}

const JSON: Typing = Typing {
    pairs: &[
        Pair {
            open: '{',
            close: '}',
            newline: true,
        },
        Pair {
            open: '[',
            close: ']',
            newline: true,
        },
        Pair {
            open: '(',
            close: ')',
            newline: false,
        },
        Pair {
            open: '"',
            close: '"',
            newline: false,
        },
    ],
    close_before: ",]}",
    word_chars: "#",
    indent_nodes: &[("object", "}"), ("array", "]")],
    string_nodes: &["string"],
};

/// The typing facts of the language named `name`. Only the JSON tab types code so far; the
/// editor tab brings the other languages (S4.3c).
pub fn typing_for(name: &str) -> Option<&'static Typing> {
    match name {
        "JSON" => Some(&JSON),
        _ => None,
    }
}

impl Typing {
    fn is_word(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '_' || self.word_chars.contains(c)
    }

    /// The char that closes `c` typed at `at`, when the pair closes itself there: not inside a
    /// string, only before a space, a line end, the end or a char of `close_before`, and a quote
    /// not right after a word char.
    pub fn closer(&self, text: &str, at: usize, c: char, in_string: bool) -> Option<char> {
        let pair = self.pairs.iter().find(|pair| pair.open == c)?;
        if in_string {
            return None;
        }
        let next = text[at..].chars().next();
        if !next.is_none_or(|next| next.is_whitespace() || self.close_before.contains(next)) {
            return None;
        }
        let after_word = text[..at].chars().next_back().is_some_and(|c| self.is_word(c));
        if pair.open == pair.close && after_word {
            return None;
        }
        Some(pair.close)
    }

    /// The pair that surrounds a selection when its opening char is typed over it.
    pub fn surround(&self, c: char) -> Option<char> {
        self.pairs
            .iter()
            .find(|pair| pair.open == c)
            .map(|pair| pair.close)
    }

    /// Whether the caret at `at` sits between the two chars of a pair that opens a line of its
    /// own on Enter.
    pub fn expands(&self, text: &str, at: usize) -> bool {
        let before = text[..at].chars().next_back();
        let after = text[at..].chars().next();
        self.pairs
            .iter()
            .any(|pair| pair.newline && before == Some(pair.open) && after == Some(pair.close))
    }

    /// The indent of `row` of `text` (parsed as `tree`), from the row above that is not blank: one
    /// level more when a block of `indent_nodes` starts on that row and goes on past this row's
    /// start, the indent of a block's first row when the block ended between the two rows, else
    /// the same.
    pub fn indent_for(&self, text: &str, tree: &Tree, row: usize) -> String {
        let lines: Vec<&str> = text.split('\n').collect();
        let Some(above) = (0..row.min(lines.len())).rev().find(|&at| !blank(lines[at])) else {
            return String::new();
        };
        let start_of = |at: usize| -> usize { lines[..at].iter().map(|line| line.len() + 1).sum() };
        let (row_start, above_start) = (start_of(row), start_of(above));
        let blocks = self.blocks(tree, row);
        let deeper = blocks
            .iter()
            .any(|&(first, end)| first == above && end > row_start);
        if deeper {
            return format!("{}{}", lead(lines[above]), " ".repeat(TAB_SIZE));
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
            if let Some((_, end)) = self
                .indent_nodes
                .iter()
                .find(|(kind, _)| *kind == node.kind())
            {
                let children: Vec<Node> = node.children(&mut cursor).collect();
                let end_at = children
                    .iter()
                    .rev()
                    .find(|child| child.kind() == *end && !child.is_missing())
                    .map_or(node.end_byte(), |child| child.start_byte());
                out.push((node.start_position().row, end_at));
            }
            nodes.extend(node.children(&mut cursor));
        }
        out
    }

    /// Whether `at` is inside (not at an edge of) a string node of `tree`.
    pub fn in_string(&self, tree: &Tree, at: usize) -> bool {
        let mut node = tree.root_node().descendant_for_byte_range(at, at);
        while let Some(found) = node {
            if self.string_nodes.contains(&found.kind())
                && found.start_byte() < at
                && at < found.end_byte()
            {
                return true;
            }
            node = found.parent();
        }
        false
    }
}

#[cfg(test)]
#[path = "code_rules_tests.rs"]
mod tests;
