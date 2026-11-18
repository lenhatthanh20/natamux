//! The input method contract of the own text input (plan 6.3.6): what Telex, VNI, EVKey,
//! OpenKey, Kotoeri and the platform's text input ask and send. Ranges are UTF-16 units.

use std::ops::Range;

use gpui::{
    Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window, point, size,
};

use super::text_input::TextInput;
use crate::text::model::EditKind;
use crate::text::motion::{from_utf16, to_utf16};

impl TextInput {
    fn utf16(&self, range: &Range<usize>) -> Range<usize> {
        let text = self.model.text();
        to_utf16(text, range.start)..to_utf16(text, range.end)
    }

    fn bytes(&self, range: &Range<usize>) -> Range<usize> {
        let text = self.model.text();
        let start = from_utf16(text, range.start);
        start..from_utf16(text, range.end).max(start)
    }
}

impl EntityInputHandler for TextInput {
    /// Clips the range to whole chars and says which range it gave.
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.bytes(&range);
        *adjusted = Some(self.utf16(&range));
        Some(self.model.text()[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let selection = self.model.selection();
        Some(UTF16Selection {
            range: self.utf16(&selection.range()),
            reversed: selection.reversed(),
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.model.marked().map(|range| self.utf16(&range))
    }

    /// Keeps the text and drops the mark; the composition's undo step ends.
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.model.set_marked(None);
        self.model.seal();
        cx.notify();
    }

    /// With marked text, the text replaces it whatever the range says. Else an empty or no range
    /// puts the text over the selection, and any other range is replaced where it is (EVKey,
    /// OpenKey and the Accessibility Keyboard send those).
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let marked = self.model.marked();
        let at_selection = range.as_ref().is_none_or(|range| range.is_empty());
        if marked.is_none() && at_selection && self.type_code(text, cx) {
            return;
        }
        let (range, kind) = match (marked, range) {
            (Some(marked), _) => (marked, EditKind::Composing),
            (None, Some(range)) if !range.is_empty() => (self.bytes(&range), EditKind::Typing),
            (None, _) => (self.model.selection().range(), EditKind::Typing),
        };
        let now = self.now(cx);
        let edited = self.model.edit(range, text, kind, now);
        self.model.set_marked(None);
        if kind == EditKind::Composing {
            self.model.seal();
        }
        self.changed(edited, cx);
    }

    /// The range counts from the start of the marked text when there is one. The new text is
    /// marked, and `selected` (from the start of the new text) is the new selection.
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only {
            return;
        }
        let marked = self.model.marked();
        let range = match (range, marked) {
            (Some(range), Some(marked)) => {
                let start = to_utf16(self.model.text(), marked.start);
                self.bytes(&(start + range.start..start + range.end))
            }
            (Some(range), None) => self.bytes(&range),
            (None, Some(marked)) => marked,
            (None, None) => self.model.selection().range(),
        };
        let now = self.now(cx);
        let edited = self
            .model
            .edit(range.clone(), text, EditKind::Composing, now);
        let inserted = range.start..range.start + crate::text::model::normalize(text).len();
        if inserted.is_empty() {
            self.model.set_marked(None);
            self.model.seal();
        } else {
            self.model.set_marked(Some(inserted.clone()));
        }
        if let Some(selected) = selected {
            let start = to_utf16(self.model.text(), inserted.start);
            let selected = self.bytes(&(start + selected.start..start + selected.end));
            self.model.select(selected.start, selected.end);
        }
        self.changed(edited, cx);
    }

    /// The rect of the char at the start of the range: one em wide and one line high, where the
    /// candidate window of an input method goes.
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.last_bounds?;
        let laid = self.layout(window, cx);
        let at = self.to_display(from_utf16(self.model.text(), range.start));
        let at = laid.position(at, false);
        Some(Bounds::new(
            point(
                bounds.left() + laid.inset + at.x - self.scroll.x,
                bounds.top() + at.y - self.scroll.y,
            ),
            size(laid.em, laid.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let (index, _) = self.index_at(at, window, cx)?;
        Some(to_utf16(self.model.text(), index))
    }

    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        !self.read_only
    }
}
