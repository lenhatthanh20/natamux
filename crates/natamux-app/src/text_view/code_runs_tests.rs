//! The lines a code view asks colours for on its first draw.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;
use std::time::Duration;

use gpui::TestAppContext;
use ui::prelude::*;

use crate::app::test_views;
use crate::text_view::code_view::CodeView;

/// A code view with more than two blocks of rows in view.
struct Host {
    view: CodeView,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(400.)).h(px(800.)).child(self.view.text_box())
    }
}

#[gpui::test]
fn a_new_code_view_lays_out_once_on_its_first_draw(cx: &mut TestAppContext) {
    let text: String = (0..200).map(|n| format!("let a{n} = {n};\n")).collect();
    let (host, _, cx) = test_views::init_app(cx, move |_, window, cx| {
        let view = CodeView::editor("x.rs", &text, window, cx);
        cx.new(|_| Host { view })
    });
    cx.executor().advance_clock(Duration::from_millis(50));
    cx.run_until_parked();
    let input = host.read_with(cx, |host, _| host.view.input().clone());
    input.update_in(cx, |input, window, cx| {
        let bounds = input.last_bounds.expect("the view was drawn");
        // The first draw again, as `prepaint` lays it out: before and after the scroll clamp.
        input.laid = None;
        let first = input.layout(window, cx);
        input.clamp_scroll(&first, bounds);
        let second = input.layout(window, cx);
        let lines = second.key.lines.clone().expect("the text has a tree");
        assert!(lines.end > 40, "more than the top two blocks: {lines:?}");
        assert!(Rc::ptr_eq(&first, &second), "laid out once");
    });
}
