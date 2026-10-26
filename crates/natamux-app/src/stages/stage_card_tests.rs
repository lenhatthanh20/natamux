#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::chips::pass_row;
use crate::app::test_views;

const WIDTH: f32 = 240.;
const LONG: &str = "Sections Goal, Scope, Out of scope, Acceptance criteria and Code map";

/// One evidence row in a box of a fixed width.
struct Evidence {
    pass: &'static str,
}

impl Render for Evidence {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(px(WIDTH)).child(pass_row(self.pass))
    }
}

/// The bounds of the drawn row and of its text.
fn drawn(pass: &'static str, cx: &mut TestAppContext) -> [gpui::Bounds<Pixels>; 2] {
    let (_view, cx) = cx.add_window_view(|_, _| Evidence { pass });
    cx.run_until_parked();
    ["evidence-row", "evidence-text"].map(|selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} was not drawn"))
    })
}

#[gpui::test]
fn a_long_pass_wraps_inside_the_card(cx: &mut TestAppContext) {
    // A long pass kept its text width and ran past the card's edge (T-0190).
    test_views::init(cx);
    let [_, short] = drawn("spec.md was written", cx);
    let [row, long] = drawn(LONG, cx);
    assert!(row.right() <= px(WIDTH));
    assert!(long.right() <= row.right());
    assert!(long.size.height > short.size.height * 1.5);
}

/// The source of `finished_card`, so the layout can be checked without a window.
fn finished_card_source() -> &'static str {
    let source = include_str!("stage_card.rs");
    let start = source.find("fn finished_card(").expect("finished_card");
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("end of finished_card");
    &rest[..end]
}

#[test]
fn a_finished_card_has_its_chevron_on_the_title_line() {
    // Beside a top-aligned head the chevron sat above the stage name (T-0169).
    let source = finished_card_source();
    assert!(source.contains("title_row(input, Some(chevron), cx)"));
    assert!(!source.contains("items_start"));
}
