//! The input method of an own terminal (docs/plan-mit-license.md 6.2.6): text it commits goes
//! to the shell as typed, text it composes shows at the cursor and reaches the shell only once
//! committed. A terminal has no text of its own to offer, so the selection is empty at 0.

use std::ops::Range;

use gpui::{App, Bounds, Entity, InputHandler, Pixels, Point, UTF16Selection, Window, point};

use super::view::TermView;

pub struct TermInput {
    view: Entity<TermView>,
}

impl TermInput {
    pub fn new(view: Entity<TermView>) -> Self {
        TermInput { view }
    }
}

impl InputHandler for TermInput {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        let marked = self.view.read(cx).marked.as_ref()?;
        Some(0..marked.encode_utf16().count())
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view
            .update(cx, |view, cx| view.commit_text(text, window, cx));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.view
            .update(cx, |view, cx| view.set_marked(text, window, cx));
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.view
            .update(cx, |view, cx| view.set_marked("", window, cx));
    }

    /// The cursor's cell, moved right by a cell per character before the range.
    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let laid = self.view.read(cx).laid?;
        let cursor = laid.cursor?;
        let shift = laid.cell_width * range_utf16.start as f32;
        Some(Bounds {
            origin: point(cursor.origin.x + shift, cursor.origin.y),
            ..cursor
        })
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }

    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}
