//! The syntax colours of a code view (plan 6.4): the tree follows each edit, and the highlight
//! query runs only over the lines around the shown ones, cached until the colours or those lines
//! change.

use std::ops::Range;

use gpui::{App, TextRun};
use theme::ActiveTheme;
use tree_sitter::InputEdit;

use super::highlight::Highlights;
use super::text_input::TextInput;
use crate::text::motion;

/// The query window moves by whole blocks of this many lines, so a scroll lays the text out again
/// only when it crosses a block.
const BLOCK: usize = 100;

/// The runs of the last query: the colour count, the theme and the lines they are of.
#[derive(Default)]
pub(super) struct RunCache {
    colors: u64,
    theme: gpui::SharedString,
    lines: Option<Range<usize>>,
    runs: Vec<(Range<usize>, String)>,
}

/// The edit that turns `old` into `new`, as one change between their common start and end.
pub(super) fn input_edit(old: &str, new: &str) -> InputEdit {
    let prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    // Whole chars only: a common byte may be the last byte of two different chars. The bytes at
    // both ends are the same in the two texts, so a char boundary of `old` is one of `new` too.
    let prefix = motion::floor_boundary(old, prefix);
    let mut suffix = suffix;
    while !old.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    InputEdit {
        start_byte: prefix,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: ts_point(old, prefix),
        old_end_position: ts_point(old, old_end),
        new_end_position: ts_point(new, new_end),
    }
}

fn ts_point(text: &str, at: usize) -> tree_sitter::Point {
    let before = &text.as_bytes()[..at];
    let row = before.iter().filter(|b| **b == b'\n').count();
    let line = before
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |at| at + 1);
    tree_sitter::Point::new(row, at - line)
}

/// The byte range of `lines` (line indices, the end left out) in `text`.
pub(super) fn line_bytes(text: &str, lines: &Range<usize>) -> Range<usize> {
    let mut starts = std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1));
    let start = starts.by_ref().nth(lines.start).unwrap_or(text.len());
    let end = if lines.end > lines.start {
        starts
            .nth(lines.end - lines.start - 1)
            .unwrap_or(text.len())
    } else {
        start
    };
    start..end.max(start)
}

impl TextInput {
    /// The lines whose colours a layout asks for: the shown lines and a block more on each side,
    /// as whole blocks. None when there is nothing to colour (no language or no tree yet).
    pub(super) fn query_lines(&self) -> Option<Range<usize>> {
        // A view with nothing to colour keeps one layout for all its lines.
        self.code.as_ref()?.syntax.as_ref()?.tree()?;
        let Some(laid) = self.laid.as_ref() else {
            // The first layout: the top lines, so a view drawn at the top lays out once.
            return Some(0..2 * BLOCK);
        };
        let bounds = self.last_bounds?;
        let line_height = laid.line_height.max(gpui::px(1.));
        let row = |y: gpui::Pixels| (y / line_height).max(0.) as usize;
        let first = laid.line_index_of_row(row(self.scroll.y));
        let last = laid.line_index_of_row(row(self.scroll.y + bounds.size.height));
        let start = (first / BLOCK).saturating_sub(1) * BLOCK;
        Some(start..(last / BLOCK + 2) * BLOCK)
    }

    /// The runs of the coloured text, from `base`; None when the text has no colours (no
    /// language, no tree yet, or the shown text is not the text). Only `lines` are coloured
    /// (every line for None); the others keep the base run.
    pub(super) fn code_runs(
        &self,
        base: &TextRun,
        len: usize,
        lines: Option<Range<usize>>,
        cx: &App,
    ) -> Option<Vec<TextRun>> {
        let code = self.code.as_ref()?;
        let syntax = code.syntax.as_ref()?;
        let tree = syntax.tree()?;
        let text = self.model.text();
        if text.len() != len {
            return None;
        }
        let highlights = Highlights::of(syntax.lang()).ok()?;
        let theme_name = cx.theme().name.clone();
        let theme = cx.theme().syntax();
        let mut cache = code.run_cache.borrow_mut();
        let stale = cache.colors != code.colors || cache.theme != theme_name;
        if stale || cache.lines != lines || cache.runs.is_empty() {
            let bytes = lines
                .as_ref()
                .map_or(0..len, |lines| line_bytes(text, lines));
            let keys = highlights.theme_keys(|key| theme.style_for_name(key).is_some());
            *cache = RunCache {
                colors: code.colors,
                theme: theme_name,
                lines: lines.clone(),
                runs: highlights.runs(tree, text, bytes, &keys),
            };
        }
        let mut runs = Vec::new();
        let mut at = 0;
        for (range, key) in &cache.runs {
            // An older tree, moved by the edits, may cut into a char.
            let start = motion::floor_boundary(text, range.start.min(len)).max(at);
            let end = motion::floor_boundary(text, range.end.min(len));
            if start >= end {
                continue;
            }
            if start > at {
                runs.push(TextRun {
                    len: start - at,
                    ..base.clone()
                });
            }
            let style = theme.style_for_name(key).unwrap_or_default();
            let mut font = base.font.clone();
            if let Some(weight) = style.font_weight {
                font.weight = weight;
            }
            if let Some(slant) = style.font_style {
                font.style = slant;
            }
            runs.push(TextRun {
                len: end - start,
                font,
                color: style.color.unwrap_or(base.color),
                ..base.clone()
            });
            at = end;
        }
        if at < len {
            runs.push(TextRun {
                len: len - at,
                ..base.clone()
            });
        }
        Some(runs)
    }
}
