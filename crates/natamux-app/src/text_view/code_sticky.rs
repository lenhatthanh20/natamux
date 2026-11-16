//! The pinned rows of a code view under the pointer (S4.3c), as Zed's sticky scroll treated
//! them: the row under the pointer is tinted (`code_gutter::paint_sticky`), and a left press on
//! it puts the caret on its line at the pointer's x and scrolls that line to the place the row
//! was pinned at. Over the rest of the gutter the pointer is an arrow (`element.rs`).

use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Hitbox, HitboxBehavior, MouseMoveEvent, Pixels,
    Point, Window, point, px, size,
};

use super::code_gutter::StickyPaint;
use super::element::Layout;
use super::text_input::TextInput;

/// The pinned row under `position`, as its index in the rows whose tops are `tops`. Where two
/// rows meet while one is pushed up, the deeper one takes the pointer, as it lay on top for the
/// pointer in Zed.
pub(super) fn sticky_under(
    tops: &[Pixels],
    (left, right): (Pixels, Pixels),
    line_height: Pixels,
    position: Point<Pixels>,
) -> Option<usize> {
    tops.iter().rposition(|top| {
        Bounds::from_corners(point(left, *top), point(right, *top + line_height))
            .contains(&position)
    })
}

/// The tops of `rows`.
pub(super) fn tops(rows: &[StickyPaint]) -> Vec<Pixels> {
    rows.iter().map(|row| row.top).collect()
}

/// What a code view puts over its box in prepaint for the pointer: a hitbox of the box, for the
/// pinned row under the pointer, and one of its gutter, for the arrow.
pub(super) struct Pointer {
    area: Hitbox,
    gutter: Option<Hitbox>,
}

impl Pointer {
    /// The hitboxes of a code view laid out as `laid` over `bounds`; none for a plain box.
    pub(super) fn insert(
        input: &TextInput,
        laid: &Layout,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) -> Option<Self> {
        input.code.as_ref()?;
        let area = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let gutter = (laid.key.gutter > px(0.)).then(|| {
            let gutter = Bounds::new(bounds.origin, size(laid.key.gutter, bounds.size.height));
            window.insert_hitbox(gutter, HitboxBehavior::Normal)
        });
        Some(Self { area, gutter })
    }

    /// In paint: the pinned row (with its top in `tops`) under the pointer, from the box's left
    /// to `right`. Off the pinned rows the gutter shows an arrow; a frame is asked for when the
    /// pointer moves on or off a pinned row, so the tint and the arrow follow it.
    pub(super) fn paint(
        &self,
        tops: Vec<Pixels>,
        (left, right): (Pixels, Pixels),
        line_height: Pixels,
        window: &mut Window,
    ) -> Option<usize> {
        let area = self.area.clone();
        let under = move |position: Point<Pixels>, window: &Window| {
            if !area.is_hovered(window) {
                return None;
            }
            sticky_under(&tops, (left, right), line_height, position)
        };
        let hovered = under(window.mouse_position(), window);
        if hovered.is_none()
            && let Some(gutter) = &self.gutter
        {
            window.set_cursor_style(CursorStyle::Arrow, gutter);
        }
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, _| {
            if phase == DispatchPhase::Bubble && under(event.position, window) != hovered {
                window.refresh();
            }
        });
        hovered
    }
}

impl TextInput {
    /// A press at `position` on a pinned row: the offset of the text under the pointer on the
    /// row's line, with the view scrolled so that line shows where the row was pinned. None off
    /// the pinned rows.
    pub(super) fn sticky_press(
        &mut self,
        position: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let laid = self.layout(window, cx);
        let rows = self.sticky_paint(&laid, bounds);
        let edges = (bounds.left(), bounds.right());
        let place = sticky_under(&tops(&rows), edges, laid.line_height, position)?;
        let row = rows.get(place)?.row;
        let x = position.x - bounds.left() - laid.inset + self.scroll.x;
        let (at, _) = laid.index_in_row(row, x);
        self.scroll.y = laid.line_height * row.saturating_sub(place) as f32;
        Some(self.text_offset(at))
    }
}
