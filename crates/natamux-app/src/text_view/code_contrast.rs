//! In a code view, text over a painted background keeps an APCA contrast of at least 45 with the
//! colour under it, as in Zed's editor (its `minimum_contrast_for_highlights` default): the
//! matching brackets, the other places of the selected text, the find matches and the selection.
//! Where they overlap, their colours lie over the editor background in that order. The contrast
//! rule itself is the terminal's (`terminals/term/contrast.rs`).

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
        let mut marks = Vec::new();
        if code.brackets_version == version {
            marks.extend(code.brackets.iter().map(|range| (shown(range), highlight)));
        }
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
            let player = cx.theme().players().local();
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
    let under = |at: usize| {
        let mut covered = marks
            .iter()
            .filter(|(range, _)| range.contains(&at))
            .peekable();
        covered.peek()?;
        Some(covered.fold(background, |color, (_, mark)| color.blend(*mark)))
    };
    let mut out = Vec::with_capacity(runs.len() + cuts.len());
    let mut at = 0;
    for run in runs {
        let end = at + run.len;
        let inner = cuts.iter().copied().filter(|cut| at < *cut && *cut < end);
        let mut start = at;
        for cut in inner.chain(std::iter::once(end)) {
            let color = under(start).map_or(run.color, |under| {
                with_minimum_contrast(run.color, under, MINIMUM)
            });
            out.push(TextRun {
                len: cut - start,
                color,
                ..run.clone()
            });
            start = cut;
        }
        at = end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::font;

    /// A grey of lightness `l`.
    fn grey(l: f32) -> Hsla {
        Hsla {
            h: 0.,
            s: 0.,
            l,
            a: 1.,
        }
    }

    fn run(len: usize, color: Hsla) -> TextRun {
        TextRun {
            len,
            font: font(".ZedMono"),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    #[test]
    fn text_under_a_mark_gets_the_least_contrast_and_the_rest_keeps_its_colour() {
        let (dark, text) = (grey(0.15), grey(0.4));
        let marks = [(2..5, grey(0.35))];
        let out = with_contrast(vec![run(8, text)], &marks, dark);
        let lens: Vec<usize> = out.iter().map(|run| run.len).collect();
        assert_eq!(lens, vec![2, 3, 3]);
        assert_eq!(out[0].color, text);
        assert_eq!(out[2].color, text);
        assert_ne!(out[1].color, text);
    }

    #[test]
    fn no_marks_leave_the_runs_as_they_are() {
        let runs = vec![run(3, grey(0.5))];
        let out = with_contrast(runs.clone(), &[], grey(0.));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].color, runs[0].color);
    }
}
