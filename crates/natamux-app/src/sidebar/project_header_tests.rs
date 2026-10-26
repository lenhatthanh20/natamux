//! A list lays each line out as a root: the project header and the rows under a task must still
//! fill its width (T-0184).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{ListAlignment, ListState, TestAppContext, list};
use ui::prelude::*;

use super::{header_row, under_guide};
use crate::app::test_views;

const WIDTH: f32 = 300.;

/// A project header with one button, then a row under a task.
struct Lines {
    list: ListState,
}

impl Render for Lines {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = cx.processor(|_, index: usize, _, cx| {
            if index == 0 {
                let fold = div().flex_1().min_w_0().h_full().into_any_element();
                let button = div()
                    .size(px(16.))
                    .debug_selector(|| "header-button".into())
                    .into_any_element();
                header_row(
                    "header".into(),
                    gpui::transparent_black(),
                    fold,
                    vec![button],
                    cx,
                )
            } else {
                let row = div()
                    .w_full()
                    .h(px(24.))
                    .debug_selector(|| "guided-row".into())
                    .into_any_element();
                under_guide(row, cx)
            }
        });
        div()
            .w(px(WIDTH))
            .h(px(200.))
            .child(list(self.list.clone(), rows).size_full())
    }
}

#[gpui::test]
fn header_buttons_and_rows_under_a_task_reach_the_right_edge(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_lines, cx) = cx.add_window_view(|_, _| Lines {
        list: ListState::new(2, ListAlignment::Top, px(0.)),
    });
    cx.run_until_parked();

    let button = cx
        .debug_bounds("header-button")
        .expect("the header button was not drawn");
    // Inside the header's 1 px border and 2 px end padding.
    assert_eq!(button.right(), px(WIDTH - 3.));

    let row = cx
        .debug_bounds("guided-row")
        .expect("the row under the task was not drawn");
    // 11 px of indent, then the 1 px guide.
    assert_eq!(row.left(), px(12.));
    assert_eq!(row.right(), px(WIDTH));
}
