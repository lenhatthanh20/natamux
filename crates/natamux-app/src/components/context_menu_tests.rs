//! The picking rules of the kit's `ContextMenu` (plan S8.3), on menus built in a window of their
//! own: which row a menu picks when it takes the focus (with and without accessibility support),
//! that Enter with nothing picked closes it, and how Up and Down skip headers and separators and
//! wrap. The keys of an app menu are in `menu_tests.rs` (T23).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;

use gpui::{DismissEvent, Entity, Focusable, TestAppContext, VisualTestContext};
use ui::menu::{Confirm, SelectFirst, SelectNext, SelectPrevious};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use crate::app::test_views;

fn focus_menu(menu: &Entity<ContextMenu>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&menu.focus_handle(cx), cx));
    cx.run_until_parked();
}

fn selected(menu: &Entity<ContextMenu>, cx: &mut VisualTestContext) -> Option<usize> {
    menu.read_with(cx, |menu, _| menu.selected_index())
}

/// 0 header, 1 First, 2 separator, 3 Checked.
fn check_focus_selection(cx: &mut TestAppContext, a11y_enabled: bool) {
    if !a11y_enabled {
        cx.disable_accessibility();
    }
    test_views::init(cx);
    let window = cx
        .update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                ContextMenu::build(window, cx, |menu, _, _| {
                    menu.header("Options")
                        .entry("First", None, |_, _| {})
                        .separator()
                        .item(
                            ContextMenuEntry::new("Checked").toggleable(IconPosition::Start, true),
                        )
                })
            })
        })
        .expect("a window opens");
    let menu = window.root(cx).expect("the menu is the root");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, _| {
        assert_eq!(window.is_a11y_enabled(), a11y_enabled);
        window.activate_window();
    });
    cx.run_until_parked();
    assert_eq!(selected(&menu, &mut cx), None);

    // Taking the focus picks the checked row, but only with accessibility support.
    focus_menu(&menu, &mut cx);
    assert_eq!(selected(&menu, &mut cx), a11y_enabled.then_some(3));

    // Down right after opening picks the first row in both cases.
    cx.dispatch_action(SelectNext);
    assert_eq!(selected(&menu, &mut cx), Some(1));
    cx.dispatch_action(SelectNext);
    assert_eq!(selected(&menu, &mut cx), Some(3));
    cx.dispatch_action(SelectPrevious);
    assert_eq!(selected(&menu, &mut cx), Some(1));

    // Taking the focus again keeps a pick.
    focus_menu(&menu, &mut cx);
    assert_eq!(selected(&menu, &mut cx), Some(1));

    menu.update(&mut cx, |menu, cx| {
        menu.clear_selected();
        cx.notify();
    });
    focus_menu(&menu, &mut cx);
    assert_eq!(selected(&menu, &mut cx), a11y_enabled.then_some(3));
}

#[gpui::test]
fn focus_selection_with_accessibility(cx: &mut TestAppContext) {
    check_focus_selection(cx, true);
}

#[gpui::test]
fn focus_selection_without_accessibility(cx: &mut TestAppContext) {
    check_focus_selection(cx, false);
}

#[gpui::test]
fn confirm_with_nothing_picked_closes_the_menu_and_runs_nothing(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let runs = Rc::new(Cell::new(0));
    let menu = cx.update(|window, cx| {
        let runs = runs.clone();
        ContextMenu::build(window, cx, move |menu, _, _| {
            menu.header("Header")
                .entry("Entry", None, move |_, _| runs.set(runs.get() + 1))
        })
    });
    let dismissed = Rc::new(Cell::new(0));
    let _subscription = cx.update(|_, cx| {
        let dismissed = dismissed.clone();
        cx.subscribe(&menu, move |_, _: &DismissEvent, _| {
            dismissed.set(dismissed.get() + 1)
        })
    });
    menu.update_in(cx, |menu, window, cx| {
        assert_eq!(menu.selected_index(), None);
        menu.confirm(&Confirm, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(dismissed.get(), 1);
    assert_eq!(runs.get(), 0);
}

#[gpui::test]
fn up_and_down_skip_headers_and_separators_and_wrap(cx: &mut TestAppContext) {
    let cx = cx.add_empty_window();
    let menu = cx.update(|window, cx| {
        ContextMenu::build(window, cx, |menu, _, _| {
            menu.header("First header")
                .separator()
                .entry("First entry", None, |_, _| {})
                .separator()
                .separator()
                .entry("Last entry", None, |_, _| {})
                .header("Last header")
        })
    });
    // Rows: 0 header, 1 separator, 2 First entry, 3 and 4 separators, 5 Last entry, 6 header.
    let step = |cx: &mut VisualTestContext, which: &str| {
        menu.update_in(cx, |menu, window, cx| match which {
            "first" => menu.select_first(&SelectFirst, window, cx),
            "next" => menu.select_next(&SelectNext, window, cx),
            _ => menu.select_previous(&SelectPrevious, window, cx),
        });
        menu.read_with(cx, |menu, _| menu.selected_index())
    };
    assert_eq!(menu.read_with(cx, |menu, _| menu.selected_index()), None);
    assert_eq!(step(cx, "first"), Some(2));
    assert_eq!(step(cx, "next"), Some(5));
    assert_eq!(step(cx, "next"), Some(2));
    assert_eq!(step(cx, "previous"), Some(5));
    assert_eq!(step(cx, "previous"), Some(2));
    assert_eq!(step(cx, "previous"), Some(5));
    assert_eq!(step(cx, "next"), Some(2));
}
