//! The commands of a code view past typing (`code_commands_tests.rs` holds the old editor's
//! results):
//! - Mod+/ comments the lines of the selection, or uncomments them when each one is commented;
//! - Ctrl+Shift+Up grows the selection to the next larger node of the syntax tree, and
//!   Ctrl+Shift+Down goes back one grow;
//! - a closing word or bracket typed at the start of a line (`}`, `fi`, `else`) moves the line
//!   back to the indent of the line that opened its block.

use std::ops::Range;
use std::time::Instant;

use gpui::{Context, Window};
use tree_sitter::{Node, Tree};

use super::code_rules::Typing;
use super::code_view::Code;
use super::text_input::TextInput;
use crate::text::minimal::Edit;
use crate::text::model::EditKind;
use crate::text::motion;

/// One edit of a toggle, and how an offset at its start moves: `after` puts it after the text
/// that comes in, as a line comment start does to the caret before it.
struct Change {
    edit: Edit,
    after: bool,
}

/// Where `at` lands once `changes` (front to back, ranges of the text before) are made.
fn moved(changes: &[Change], at: usize) -> usize {
    let mut shift: isize = 0;
    for change in changes {
        let range = &change.edit.range;
        let inserted = change.edit.text.len() as isize;
        if at > range.end || (at == range.end && range.is_empty() && change.after) {
            shift += inserted - range.len() as isize;
        } else if at > range.start {
            // Inside a removed part: to where it was.
            return (range.start as isize + shift) as usize;
        }
    }
    (at as isize + shift) as usize
}

/// The leading spaces and tabs of `line`.
fn indent_len(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// The line comment changes for the lines of `block` (a whole-lines range of `text`).
fn line_changes(text: &str, block: Range<usize>, start: &str) -> Vec<Change> {
    let mark = start.trim_end();
    let mut lines = Vec::new();
    let mut at = block.start;
    for line in text[block.clone()].split('\n') {
        lines.push((at, line));
        at += line.len() + 1;
    }
    let commented = lines
        .iter()
        .filter(|(_, line)| !line.trim().is_empty())
        .all(|(_, line)| line.trim_start().starts_with(mark));
    if commented {
        return lines
            .iter()
            .filter(|(_, line)| line.trim_start().starts_with(mark))
            .map(|(start_at, line)| {
                let from = start_at + indent_len(line);
                let rest = &text[from + mark.len()..start_at + line.len()];
                let len = mark.len() + usize::from(rest.starts_with(' '));
                Change {
                    edit: Edit {
                        range: from..from + len,
                        text: String::new(),
                    },
                    after: false,
                }
            })
            .collect();
    }
    let column = lines
        .iter()
        .map(|(_, line)| indent_len(line))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|(start_at, _)| Change {
            edit: Edit {
                range: start_at + column..start_at + column,
                text: start.to_owned(),
            },
            after: true,
        })
        .collect()
}

/// The block comment changes for the lines of `block`: around their text, or off it when it is
/// one comment already.
fn block_changes(text: &str, block: Range<usize>, (open, close): (&str, &str)) -> Vec<Change> {
    let from = block.start + indent_len(&text[block.clone()]);
    let inner = text[from..block.end].trim_end();
    let to = from + inner.len();
    let edit = |range: Range<usize>, text: &str, after: bool| Change {
        edit: Edit {
            range,
            text: text.to_owned(),
        },
        after,
    };
    if inner.len() >= open.len() + close.len() && inner.starts_with(open) && inner.ends_with(close)
    {
        return vec![
            edit(from..from + open.len(), "", false),
            edit(to - close.len()..to, "", false),
        ];
    }
    vec![
        edit(from..from, open, true),
        edit(block.end..block.end, close, false),
    ]
}

/// The deepest node on the path down from the root through the children `step` takes (the
/// first child it takes at each level) that `keep` holds for.
fn deepest<'t>(
    root: Node<'t>,
    step: impl Fn(&Node) -> bool,
    keep: impl Fn(&Node) -> bool,
) -> Option<Node<'t>> {
    let mut found = keep(&root).then_some(root);
    let mut node = root;
    loop {
        let mut cursor = node.walk();
        let Some(child) = node.children(&mut cursor).find(|child| step(child)) else {
            return found;
        };
        if keep(&child) {
            found = Some(child);
        }
        node = child;
    }
}

/// The range to grow `range` to. A selection grows to the smallest node around it that is
/// larger. A caret takes the named node that starts at it; else the smallest node it is
/// strictly inside; else the smallest node it touches.
fn grown(tree: &Tree, range: Range<usize>) -> Option<Range<usize>> {
    let root = tree.root_node();
    if !range.is_empty() {
        let around = |node: &Node| node.start_byte() <= range.start && range.end <= node.end_byte();
        return deepest(root, around, |node| node.byte_range().len() > range.len())
            .map(|node| node.byte_range());
    }
    let at = range.start;
    let right = deepest(
        root,
        |node| node.start_byte() <= at && at < node.end_byte(),
        |node| node.start_byte() == at,
    );
    if let Some(right) = right.filter(|node| node.is_named()) {
        return Some(right.byte_range());
    }
    let inside = |node: &Node| node.start_byte() < at && at < node.end_byte();
    if let Some(node) = deepest(root, inside, inside) {
        return Some(node.byte_range());
    }
    let touches = |node: &Node| {
        node.start_byte() <= at && at <= node.end_byte() && node.end_byte() > node.start_byte()
    };
    deepest(root, touches, touches).map(|node| node.byte_range())
}

impl TextInput {
    /// Mod+/: the language's line comment start before each line of the selection, at the
    /// smallest indent among them; or off again when every line with text has it. A language
    /// with only a block comment puts it around the lines' text instead.
    pub(super) fn toggle_comments(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(toggle) = self.typing().and_then(|typing| typing.comment_toggle) else {
            return;
        };
        if self.read_only {
            return;
        }
        let selection = self.model.selection();
        let block = self.line_block(selection.range());
        let text = self.model.text();
        let changes = match toggle {
            (start, "") => line_changes(text, block, start),
            pair => block_changes(text, block, pair),
        };
        let edits: Vec<Edit> = changes.iter().map(|change| change.edit.clone()).collect();
        let (anchor, head) = (
            moved(&changes, selection.anchor),
            moved(&changes, selection.head),
        );
        self.edit_all(&edits, anchor, head, cx);
    }

    /// Ctrl+Shift+Up: the selection grows to the next larger node; the old one is kept for
    /// Ctrl+Shift+Down.
    pub(super) fn grow_selection(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.code.as_ref().and_then(Code::tree) else {
            return;
        };
        let old = self.model.selection();
        let Some(new) = grown(tree, old.range()) else {
            return;
        };
        if new == old.range() {
            return;
        }
        let reversed =
            new.start < old.range().start || (new.start == old.range().start && old.reversed());
        let version = self.model.version();
        if let Some(code) = self.code.as_mut() {
            if code.grows_version != version {
                code.grows.clear();
            }
            code.grows.push(old.range());
            code.grows_version = version;
        }
        if reversed {
            self.model.select(new.end, new.start);
        } else {
            self.model.select(new.start, new.end);
        }
        self.changed(false, cx);
    }

    /// Ctrl+Shift+Down: back to the range before the last grow, while the text is the same.
    pub(super) fn shrink_selection(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let version = self.model.version();
        let Some(range) = self
            .code
            .as_mut()
            .filter(|code| code.grows_version == version)
            .and_then(|code| code.grows.pop())
        else {
            return;
        };
        // The old range comes back facing the way the selection faces now.
        if self.model.selection().reversed() {
            self.model.select(range.end, range.start);
        } else {
            self.model.select(range.start, range.end);
        }
        self.changed(false, cx);
    }

    /// After a char typed with nothing selected: when the line's text before the caret now
    /// starts with one of the language's closing words or brackets, and the line that opened the
    /// block (the nearest line above with less indent) fits it, the line takes that line's
    /// indent. When the next char makes it no closing word any more (`fi` + `n`), the indent comes
    /// back. Part of the typing step. `version` is the text version before the char.
    pub(super) fn dedent_closer(&mut self, typing: &Typing, now: Instant, version: u64) {
        let text = self.model.text();
        let caret = self.model.head();
        let start = motion::line_start(text, caret);
        let before = &text[start..caret];
        let indent = indent_len(before);
        let opener = opener_above(text, start, indent);
        let closers = || {
            typing
                .dedent
                .iter()
                .filter(|(word, _)| starts_with_word(&before[indent..], word))
        };
        let is_closer = closers().next().is_some();
        let closes = opener
            .as_ref()
            .is_some_and(|(_, line)| closers().any(|(_, fits)| fits(line)));
        let kept = self
            .code
            .as_mut()
            .and_then(|code| code.dedented.take())
            .filter(|(line, _, at)| *line == start && *at == version);
        let target = match (kept, opener) {
            // Still the closing word of the char before: it keeps its indent.
            (Some((_, old, _)), _) if is_closer => Some((before[..indent].to_owned(), old)),
            (None, Some((lead, _))) if closes && indent > 0 => {
                Some((lead, before[..indent].to_owned()))
            }
            (Some((_, old, _)), _) => {
                self.replace_indent(start, indent, &old, caret, now);
                return;
            }
            _ => None,
        };
        let Some((target, old)) = target else {
            return;
        };
        if target != before[..indent] {
            self.replace_indent(start, indent, &target, caret, now);
        }
        let version = self.model.version();
        if let Some(code) = self.code.as_mut() {
            code.dedented = Some((start, old, version));
        }
    }

    /// The indent of the line at `start` (`indent` bytes long) becomes `target`; the caret, at
    /// `caret` in the line, moves with it.
    fn replace_indent(
        &mut self,
        start: usize,
        indent: usize,
        target: &str,
        caret: usize,
        now: Instant,
    ) {
        self.model
            .edit(start..start + indent, target, EditKind::Typing, now);
        self.model.move_to(caret - indent + target.len());
    }
}

/// The nearest line with text above the line at `start` whose indent is less than `indent`: its
/// indent and its text.
fn opener_above(text: &str, start: usize, indent: usize) -> Option<(String, &str)> {
    let mut above = start;
    while above > 0 {
        let line_start = motion::line_start(text, above - 1);
        let line = &text[line_start..above - 1];
        above = line_start;
        if !line.trim().is_empty() && indent_len(line) < indent {
            return Some((line[..indent_len(line)].to_owned(), line));
        }
    }
    None
}

/// Whether `text` starts with `word`, and a word does not go on after it.
fn starts_with_word(text: &str, word: &str) -> bool {
    text.strip_prefix(word).is_some_and(|rest| {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        !(word.ends_with(is_word) && rest.starts_with(is_word))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, changes: &[Change]) -> String {
        let mut out = text.to_owned();
        for change in changes.iter().rev() {
            out.replace_range(change.edit.range.clone(), &change.edit.text);
        }
        out
    }

    #[test]
    fn line_comments_go_at_the_smallest_indent_and_come_off_again() {
        let text = "    a\n\n      b";
        let on = line_changes(text, 0..text.len(), "// ");
        let commented = apply(text, &on);
        assert_eq!(commented, "//     a\n// \n//       b");
        assert_eq!(moved(&on, 0), 3);
        let off = line_changes(&commented, 0..commented.len(), "// ");
        assert_eq!(apply(&commented, &off), text);
    }

    #[test]
    fn a_block_comment_goes_around_the_text_of_the_lines() {
        let text = "    a b";
        let on = block_changes(text, 0..text.len(), ("/*", "*/"));
        assert_eq!(apply(text, &on), "    /*a b*/");
        assert_eq!(moved(&on, 7), 9);
        let off = block_changes("/* ab */", 0..8, ("/*", "*/"));
        assert_eq!(apply("/* ab */", &off), " ab ");
        assert_eq!(moved(&off, 5), 3);
    }
}
