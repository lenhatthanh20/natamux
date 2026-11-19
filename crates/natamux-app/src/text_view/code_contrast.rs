//! In a code view, text over a painted background keeps an APCA contrast of at least 45 with the
//! colour under it, as in Zed's editor (its `minimum_contrast_for_highlights` default): the
//! other places of the selected text, the find matches and the selection.
//! Where they overlap, their colours lie over the editor background in that order. The contrast
//! rule itself is the terminal's (`terminals/term/contrast.rs`).

use std::collections::BTreeMap;
use std::ops::Range;

use gpui::{App, Hsla, TextRun};
use theme::ActiveTheme;

use super::text_input::{Mode, TextInput};
use crate::terminals::term::contrast::with_minimum_contrast;

/// The least APCA contrast of text with a painted background under it.
const MINIMUM: f32 = 45.;

impl TextInput {
    /// The painted backgrounds under the text of a code view, in offsets of the shown text, in
    /// the order they are laid over each other. `focused`: the selection has its focused colour.
    pub(super) fn contrast_marks(&self, focused: bool, cx: &App) -> Vec<(Range<usize>, Hsla)> {
        let Some(code) = self.code.as_ref() else {
            return Vec::new();
        };
        if !matches!(self.mode, Mode::Full { .. }) {
            return Vec::new();
        }
        let colors = cx.theme().colors();
        let highlight = colors.editor_document_highlight_bracket_background;
        let version = self.model.version();
        let selection = self.model.selection().range();
        let shown = |range: &Range<usize>| self.to_display(range.start)..self.to_display(range.end);
        // The matching brackets keep their colour: Zed's editor drew their background with the
        // text, outside this rule (B2 shot `workspace-changes-diff-solarized-dark`, the `}`).
        let mut marks: Vec<(Range<usize>, Hsla)> = Vec::new();
        marks.extend(
            code.other_occurrences(selection.clone())
                .map(|range| (shown(range), highlight)),
        );
        if code.search_version == version {
            marks.extend(code.search.iter().enumerate().map(|(index, range)| {
                let color = if code.search_active == Some(index) {
                    colors.search_active_match_background
                } else {
                    colors.search_match_background
                };
                (shown(range), color)
            }));
        }
        if !selection.is_empty() {
            let player = if self.read_only {
                cx.theme().players().read_only()
            } else {
                cx.theme().players().local()
            };
            let color = if focused {
                player.selection
            } else {
                player.selection.opacity(0.5)
            };
            marks.push((shown(&selection), color));
        }
        marks.retain(|(range, _)| !range.is_empty());
        marks
    }
}

/// `runs` with the text over `marks` raised to the least contrast with the colour under it: the
/// colours of the marks there laid over `background`.
pub(super) fn with_contrast(
    runs: Vec<TextRun>,
    marks: &[(Range<usize>, Hsla)],
    background: Hsla,
) -> Vec<TextRun> {
    if marks.is_empty() {
        return runs;
    }
    let mut cuts: Vec<usize> = marks
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let under = colors_from_cuts(&cuts, marks, background);
    let mut out = Vec::with_capacity(runs.len() + cuts.len());
    let mut at = 0;
    for run in runs {
        let end = at + run.len;
        // The number of cuts at or before `start`: the last of them gives the colour under it.
        let mut next = cuts.partition_point(|cut| *cut <= at);
        let mut start = at;
        loop {
            let cut = match cuts.get(next) {
                Some(&cut) if cut < end => cut,
                _ => end,
            };
            let color = match next.checked_sub(1).and_then(|last| under.get(last)) {
                Some(Some(mark)) => with_minimum_contrast(run.color, *mark, MINIMUM),
                _ => run.color,
            };
            out.push(TextRun {
                len: cut - start,
                color,
                ..run.clone()
            });
            if cut == end {
                break;
            }
            start = cut;
            next += 1;
        }
        at = end;
    }
    out
}

/// For each of the sorted `cuts`, the colour under the text from it to the next cut: the marks
/// over that span laid over `background` in mark order, or `None` where no mark lies.
fn colors_from_cuts(
    cuts: &[usize],
    marks: &[(Range<usize>, Hsla)],
    background: Hsla,
) -> Vec<Option<Hsla>> {
    let mut events: Vec<(usize, usize, Option<Hsla>)> = Vec::with_capacity(marks.len() * 2);
    for (index, (range, color)) in marks.iter().enumerate() {
        // An empty or reversed range contains no offset, so it never lies under text.
        if range.start < range.end {
            events.push((range.start, index, Some(*color)));
            events.push((range.end, index, None));
        }
    }
    events.sort_unstable_by_key(|(at, _, _)| *at);
    let mut events = events.into_iter().peekable();
    // Keyed by mark index, so the fold below keeps the order the marks are laid in.
    let mut over: BTreeMap<usize, Hsla> = BTreeMap::new();
    let mut out = Vec::with_capacity(cuts.len());
    for &cut in cuts {
        while let Some((_, index, color)) = events.next_if(|(at, _, _)| *at == cut) {
            match color {
                Some(color) => {
                    over.insert(index, color);
                }
                None => {
                    over.remove(&index);
                }
            }
        }
        let color = if over.is_empty() {
            None
        } else {
            Some(
                over.values()
                    .fold(background, |color, mark| color.blend(*mark)),
            )
        };
        out.push(color);
    }
    out
}

#[cfg(test)]
#[path = "code_contrast_tests.rs"]
mod tests;
