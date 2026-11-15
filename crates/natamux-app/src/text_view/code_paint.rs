//! What a code view paints past the plain input (plan 6.4): indent guides, the matching
//! brackets, the other places of the selected text and the scrollbar.

use std::ops::Range;

use gpui::{App, Bounds, Hsla, Pixels, point, px};

use super::code_bar::BarBounds;
use super::element::Layout;
use super::text_input::{TAB_SIZE, TextInput};
use crate::app::appearance::Appearance;

/// What a code view paints past the plain input, in window coordinates.
#[derive(Default)]
pub(super) struct CodePaint {
    /// Each indent guide line, and whether it is active.
    pub guides: Vec<(Bounds<Pixels>, bool)>,
    /// The matching brackets and the other places of the selected text.
    pub backgrounds: Vec<Bounds<Pixels>>,
    /// The matches of find, and whether each is the active one.
    pub search: Vec<(Bounds<Pixels>, bool)>,
    /// Whitespace marks for the whole text, not only inside the selection.
    pub all_whitespace: bool,
    /// The scrollbar, hovered, dragged.
    pub bar: Option<(BarBounds, bool, bool)>,
    /// The markers on the scrollbar, painted before its thumb.
    pub markers: Vec<(Bounds<Pixels>, Hsla)>,
}

impl TextInput {
    /// The parts of a code view to paint over `bounds`, where the text starts at `origin`.
    pub(super) fn code_paint(
        &self,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        origin: gpui::Point<Pixels>,
        cx: &App,
    ) -> CodePaint {
        let Some(code) = self.code.as_ref() else {
            return CodePaint::default();
        };
        let text = self.model.text();
        let line_height = laid.line_height;
        let mut paint = CodePaint {
            all_whitespace: cx
                .try_global::<Appearance>()
                .is_some_and(|look| look.show_whitespace),
            bar: self
                .bar_bounds(laid, bounds, cx)
                .map(|bar| (bar, self.bar_hovered(), self.bar_dragged())),
            ..CodePaint::default()
        };
        if let Some((bar, _, _)) = &paint.bar {
            paint.markers = self.bar_markers(laid, bar, cx);
        }
        if code.guides {
            let found = super::code_rules::guides(text);
            let caret_row = text[..self.model.head()].matches('\n').count();
            let active = super::code_rules::active_guides(text, &found, caret_row);
            for (index, guide) in found.iter().enumerate() {
                let (Some(first), Some(last)) =
                    (laid.lines.get(guide.start), laid.lines.get(guide.end))
                else {
                    continue;
                };
                let top = origin.y + line_height * first.first_row as f32;
                let rows = last.first_row + last.row_starts.len() - first.first_row;
                let height = line_height * rows as f32;
                if top + height < bounds.top() || top > bounds.bottom() {
                    continue;
                }
                let x = origin.x + laid.em * (guide.depth * TAB_SIZE) as f32;
                paint.guides.push((
                    Bounds::new(point(x, top), gpui::size(px(1.), height)),
                    active.contains(&index),
                ));
            }
        }
        let version = self.model.version();
        let mut ranges: Vec<Range<usize>> = if code.brackets_version == version {
            code.brackets.clone()
        } else {
            Vec::new()
        };
        // The selected text itself shows only in the selection colour.
        ranges.extend(
            code.other_occurrences(self.model.selection().range())
                .cloned(),
        );
        for range in ranges {
            for rect in laid.selection_rects(range) {
                let rect = Bounds::new(rect.origin + origin, rect.size);
                if rect.bottom() >= bounds.top() && rect.top() <= bounds.bottom() {
                    paint.backgrounds.push(rect);
                }
            }
        }
        let search: &[Range<usize>] = if code.search_version == version {
            &code.search
        } else {
            &[]
        };
        for (index, range) in search.iter().enumerate() {
            for rect in laid.selection_rects(range.clone()) {
                let rect = Bounds::new(rect.origin + origin, rect.size);
                if rect.bottom() >= bounds.top() && rect.top() <= bounds.bottom() {
                    paint.search.push((rect, code.search_active == Some(index)));
                }
            }
        }
        paint
    }
}
