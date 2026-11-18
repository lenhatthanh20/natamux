//! The first parse of a big text in the code view: a caret move while it runs in the background
//! lets it finish, it does not start it again (B-10).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

use super::code_view::CodeView;
use super::text_input::TextInput;
use crate::app::test_views::{self, Opener};

/// The colour count of the code view and whether it has a tree.
fn parse_state(input: &Entity<TextInput>, cx: &mut VisualTestContext) -> (u64, bool) {
    input.read_with(cx, |input, _| {
        let code = input.code.as_ref().unwrap();
        (code.colors, code.tree().is_some())
    })
}

#[gpui::test]
fn a_caret_move_does_not_restart_a_parse_in_flight(cx: &mut TestAppContext) {
    let (_, _, cx) = test_views::init_app(cx, |_, _, cx| cx.new(Opener::new));
    // Far too big for PARSE_BUDGET on any machine (as in highlight_tests.rs).
    let text = format!("[{}0]", "1, ".repeat(400_000));
    let view = cx.update(|window, cx| CodeView::new("big.json", &text, false, window, cx));
    let input = view.input().clone();
    // The background parse runs only once the test parks, so nothing has landed yet.
    assert_eq!(parse_state(&input, cx), (1, false));
    for at in [1, 4, 7] {
        input.update(cx, |input, cx| {
            input.model.move_to(at);
            input.changed(false, cx);
        });
    }
    assert_eq!(
        parse_state(&input, cx),
        (1, false),
        "a caret move restarted the parse"
    );
    cx.run_until_parked();
    assert_eq!(parse_state(&input, cx), (2, true));
}
