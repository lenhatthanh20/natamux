//! The right-click menu of a terminal shows only what works without a Zed workspace (T-0247).

use gpui::{TestAppContext, point};
use natamux_client::TerminalEvent;
use ui::prelude::*;

use super::{apply, draw, focus_view, open, output, reset, shown_text};

/// Regression (T-0247): the menu started with New Terminal and New Center Terminal, which only a
/// Zed workspace handles, so a click did nothing. Its first item is now Copy.
#[gpui::test]
fn the_first_menu_item_copies_the_selection(cx: &mut TestAppContext) {
    let (terminal, cx) = open(cx);
    let events = vec![
        reset(),
        output("qx-menu", None),
        TerminalEvent::CaughtUp { offset: 7 },
    ];
    apply(&terminal, events, cx);
    assert!(shown_text(&terminal, cx).contains("qx-menu"));
    focus_view(&terminal, cx);

    terminal.update(cx, |terminal, cx| {
        terminal.terminal().update(cx, |term, _| term.select_all())
    });
    draw(cx);
    let view = terminal.read_with(cx, |terminal, _| terminal.view.clone());
    view.update_in(cx, |view, window, cx| {
        view.deploy_context_menu(point(px(8.), px(8.)), true, window, cx)
    });
    draw(cx);

    for name in ["menu::SelectFirst", "menu::Confirm"] {
        cx.update(|window, cx| {
            let action = cx.build_action(name, None).unwrap();
            window.dispatch_action(action, cx);
        });
        cx.run_until_parked();
        draw(cx);
    }

    let copied = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert!(
        copied.is_some_and(|text| text.contains("qx-menu")),
        "the first menu item did not copy"
    );
}
