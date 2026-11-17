//! The picking rules of the kit's `ContextMenu` (plan S8.3): which row a menu picks when it takes
//! the focus (with and without accessibility support), that Enter with nothing picked closes it
//! and runs nothing, and how Up and Down skip headers, separators and text lines and wrap. Each
//! menu is the root of a window of its own. The keys of an app menu are in `menu_tests.rs` (T23).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Context, DismissEvent, Entity, Focusable, TestAppContext, VisualTestContext, Window};
use ui::menu::{Confirm, SelectFirst, SelectNext, SelectPrevious};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use crate::app::test_views;

/// A window whose root is the menu `fill` makes.
fn open(
    cx: &mut TestAppContext,
    fill: impl FnOnce(ContextMenu, &mut Window, &mut Context<ContextMenu>) -> ContextMenu + 'static,
) -> (Entity<ContextMenu>, VisualTestContext) {
    test_views::init(cx);
    let window = cx
        .update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                ContextMenu::build(window, cx, fill)
            })
        })
        .expect("the window opens");
    let menu = window.root(cx).expect("the menu is the root view");
    let mut window_cx = VisualTestContext::from_window(window.into(), cx);
    window_cx.update(|window, _| window.activate_window());
    window_cx.run_until_parked();
    (menu, window_cx)
}

fn picked(menu: &Entity<ContextMenu>, cx: &mut VisualTestContext) -> Option<usize> {
    menu.read_with(cx, |menu, _| menu.selected_index())
}

/// The menu loses the focus, then takes it again.
fn refocus(menu: &Entity<ContextMenu>, cx: &mut VisualTestContext) {
    let handle = menu.read_with(cx, |menu, cx| menu.focus_handle(cx));
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
}

/// Rows: 0 a text line, 1 Plain, 2 Ticked (checked), 3 separator.
fn picks_on_focus(cx: &mut TestAppContext, a11y: bool) {
    if !a11y {
        cx.disable_accessibility();
    }
    let (menu, mut cx) = open(cx, |menu, _, _| {
        menu.label("View")
            .entry("Plain", None, |_, _| {})
            .item(ContextMenuEntry::new("Ticked").toggleable(IconPosition::Start, true))
            .separator()
    });
    let on_focus = if a11y { Some(2) } else { None };
    assert_eq!(cx.update(|window, _| window.is_a11y_enabled()), a11y);
    assert_eq!(
        picked(&menu, &mut cx),
        None,
        "nothing is picked before the focus"
    );

    refocus(&menu, &mut cx);
    assert_eq!(
        picked(&menu, &mut cx),
        on_focus,
        "the checked row, with a11y only"
    );

    // From here the keys walk the same way in both cases: Down first goes to the top row.
    for (key, want) in [("down", 1), ("down", 2), ("up", 1)] {
        match key {
            "down" => cx.dispatch_action(SelectNext),
            _ => cx.dispatch_action(SelectPrevious),
        }
        assert_eq!(picked(&menu, &mut cx), Some(want), "after {key}");
    }

    refocus(&menu, &mut cx);
    assert_eq!(
        picked(&menu, &mut cx),
        Some(1),
        "a new focus keeps the pick"
    );

    menu.update(&mut cx, |menu, cx| {
        menu.clear_selected();
        cx.notify();
    });
    refocus(&menu, &mut cx);
    assert_eq!(
        picked(&menu, &mut cx),
        on_focus,
        "with no pick, the focus picks again"
    );
}

#[gpui::test]
fn focus_selection_with_accessibility(cx: &mut TestAppContext) {
    picks_on_focus(cx, true);
}

#[gpui::test]
fn focus_selection_without_accessibility(cx: &mut TestAppContext) {
    picks_on_focus(cx, false);
}

#[gpui::test]
fn confirm_with_nothing_picked_closes_the_menu_and_runs_nothing(cx: &mut TestAppContext) {
    cx.disable_accessibility();
    let ran = Rc::new(Cell::new(false));
    let ran_in_row = ran.clone();
    let (menu, mut cx) = open(cx, move |menu, _, _| {
        menu.header("Title")
            .entry("Run", None, move |_, _| ran_in_row.set(true))
    });
    let closed = Rc::new(Cell::new(0_u32));
    let closed_seen = closed.clone();
    let _watch = cx.update(|_, cx| {
        cx.subscribe(&menu, move |_, _: &DismissEvent, _| {
            closed_seen.set(closed_seen.get() + 1)
        })
    });

    assert_eq!(picked(&menu, &mut cx), None);
    menu.update_in(&mut cx, |menu, window, cx| {
        menu.confirm(&Confirm, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(closed.get(), 1, "the menu closes once");
    assert!(!ran.get(), "no row runs");
}

#[gpui::test]
fn up_and_down_skip_headers_and_separators_and_wrap(cx: &mut TestAppContext) {
    cx.disable_accessibility();
    // Rows: 0 header, 1 and 2 separators, 3 Top, 4 a text line, 5 separator, 6 Bottom,
    // 7 header, 8 separator.
    let (menu, mut cx) = open(cx, |menu, _, _| {
        menu.header("Above")
            .separator()
            .separator()
            .entry("Top", None, |_, _| {})
            .label("Between")
            .separator()
            .entry("Bottom", None, |_, _| {})
            .header("Below")
            .separator()
    });
    assert_eq!(picked(&menu, &mut cx), None);

    let walk = [
        ("first", 3),
        ("down", 6),
        ("down", 3),
        ("up", 6),
        ("up", 3),
        ("up", 6),
        ("down", 3),
    ];
    for (key, want) in walk {
        menu.update_in(&mut cx, |menu, window, cx| match key {
            "first" => menu.select_first(&SelectFirst, window, cx),
            "down" => menu.select_next(&SelectNext, window, cx),
            _ => menu.select_previous(&SelectPrevious, window, cx),
        });
        assert_eq!(picked(&menu, &mut cx), Some(want), "after {key}");
    }
}
