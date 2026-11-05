//! A scroll area's bar must not be part of what scrolls: on the scroll div it moved with the
//! content, and its box widened the range so a fitting area still scrolled (T-0286).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Point, ScrollHandle, TestAppContext};
use ui::{ScrollAxes, prelude::*};

use super::scroll_area;
use crate::app::test_views;

/// A padded area 200 x 100 with `rows` squares of 20 px, in a row or a column.
struct Area {
    rows: usize,
    sideways: bool,
    handle: ScrollHandle,
}

impl Render for Area {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let area = div()
            .id("area")
            .size_full()
            .flex()
            .px_4()
            .py_3()
            .track_scroll(&self.handle)
            .children((0..self.rows).map(|_| div().flex_none().size(px(20.))));
        let (area, axes) = if self.sideways {
            (area.flex_row().overflow_x_scroll(), ScrollAxes::Horizontal)
        } else {
            (area.flex_col().overflow_y_scroll(), ScrollAxes::Vertical)
        };
        div()
            .w(px(200.))
            .h(px(100.))
            .child(scroll_area(area, axes, &self.handle, window, cx).size_full())
    }
}

fn draw(rows: usize, sideways: bool, cx: &mut TestAppContext) -> ScrollHandle {
    test_views::init(cx);
    let handle = ScrollHandle::new();
    let shared = handle.clone();
    let (_area, cx) = cx.add_window_view(move |_, _| Area {
        rows,
        sideways,
        handle: shared,
    });
    cx.run_until_parked();
    handle
}

#[gpui::test]
fn a_fitting_column_does_not_scroll(cx: &mut TestAppContext) {
    assert_eq!(draw(2, false, cx).max_offset(), Point::default());
}

#[gpui::test]
fn a_fitting_row_does_not_scroll(cx: &mut TestAppContext) {
    assert_eq!(draw(2, true, cx).max_offset(), Point::default());
}

// 20 squares of 20 px plus the padding (12 + 12 down, 16 + 16 across), less the box.
#[gpui::test]
fn a_long_column_scrolls_its_rows_only(cx: &mut TestAppContext) {
    let handle = draw(20, false, cx);
    assert_eq!(handle.children_count(), 20);
    assert_eq!(
        handle.max_offset(),
        Point::new(px(0.), px(400. + 24. - 100.))
    );
}

#[gpui::test]
fn a_long_row_scrolls_its_rows_only(cx: &mut TestAppContext) {
    let handle = draw(20, true, cx);
    assert_eq!(handle.children_count(), 20);
    assert_eq!(handle.max_offset().x, px(400. + 32. - 200.));
}
