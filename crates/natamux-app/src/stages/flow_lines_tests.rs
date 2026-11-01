#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::JoinDot;
use crate::app::test_views;
use crate::stages::flow_rules::LinkStyle;

const WIDTH: f32 = 300.;

/// A join dot with its note, in a row as wide as the pair above it.
struct Join {
    note: &'static str,
}

impl Render for Join {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(WIDTH)).child(
            JoinDot::new(LinkStyle::Done)
                .note(self.note)
                .below(LinkStyle::Done),
        )
    }
}

#[gpui::test]
fn the_join_dot_sits_where_the_join_curves_meet(cx: &mut TestAppContext) {
    // The note's padding made its side wider and moved the dot 3 px left of the middle, where
    // the join curves meet (owner, T-0259).
    test_views::init(cx);
    for note in ["", "Review starts when both are finished"] {
        let (_view, cx) = cx.add_window_view(|_, _| Join { note });
        cx.run_until_parked();
        let dot = cx.debug_bounds("join-dot").unwrap();
        let middle = dot.left() + dot.size.width / 2.;
        assert!(
            (middle - px(WIDTH / 2.)).abs() <= px(0.5),
            "{note}: {dot:?}"
        );
    }
}
