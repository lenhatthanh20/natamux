//! A click on a row in the sidebar drawer reaches the row, not the scrim behind the drawer
//! (T-0198).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{
    ClickEvent, Entity, Modifiers, MouseDownEvent, TestAppContext, VisualTestContext, point,
};
use ui::prelude::*;

use super::drawer_layer;
use crate::app::test_views;

/// Root's drawer with one row in it: closing drops the drawer, as `set_drawer(false)` does.
struct Host {
    open: bool,
    row_clicks: usize,
    closes: usize,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = f32::from(window.viewport_size().width);
        let row = div()
            .id("row")
            .w_full()
            .h(px(24.))
            .debug_selector(|| "drawer-row".into())
            .on_click(cx.listener(|host, _: &ClickEvent, _, _| host.row_clicks += 1));
        let close = cx.listener(|host, _: &MouseDownEvent, _, cx| {
            host.open = false;
            host.closes += 1;
            cx.notify();
        });
        let drawer = self.open.then(|| drawer_layer(row, viewport, close, cx));
        div().relative().size_full().children(drawer)
    }
}

fn open_drawer(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    test_views::init(cx);
    let (host, cx) = cx.add_window_view(|_, _| Host {
        open: true,
        row_clicks: 0,
        closes: 0,
    });
    cx.run_until_parked();
    (host, cx)
}

#[gpui::test]
fn a_click_on_a_row_in_the_drawer_reaches_the_row(cx: &mut TestAppContext) {
    let (host, cx) = open_drawer(cx);
    let row = cx
        .debug_bounds("drawer-row")
        .expect("the row was not drawn");

    cx.simulate_click(row.center(), Modifiers::none());

    host.read_with(cx, |host, _| {
        assert_eq!(host.closes, 0, "the click on the row closed the drawer");
        assert_eq!(host.row_clicks, 1, "the row did not get the click");
    });
}

#[gpui::test]
fn a_click_on_the_scrim_closes_the_drawer(cx: &mut TestAppContext) {
    let (host, cx) = open_drawer(cx);
    let row = cx
        .debug_bounds("drawer-row")
        .expect("the row was not drawn");

    // The row fills the panel's width, so right of it is the scrim.
    cx.simulate_click(
        point(row.right() + px(40.), row.center().y),
        Modifiers::none(),
    );

    host.read_with(cx, |host, _| {
        assert_eq!(
            host.closes, 1,
            "the click on the scrim did not close the drawer"
        );
        assert_eq!(host.row_clicks, 0);
        assert!(!host.open);
    });
}
