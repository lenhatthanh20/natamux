//! Workflow test T23 (plan 7.5), the keys of an open menu: every app menu opens through
//! `MenuLayer::open`, which hands the menu its keys. Here one menu has each kind of line the app's
//! menus use (a header, entries, a toggled entry, a separator, a disabled entry, a submenu), and
//! each key does what it does today: the menu opens with its toggled entry picked, Up, Down,
//! Home, End, Page Up, Page Down move, Enter and Space run the entry once and close, Esc and Tab
//! close, keys with a modifier stay with the app, and the focus goes back to the opener. Right
//! opens a submenu, but its keys then reach nothing (see "Bugs found" of the MIT move).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, point};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::menu::MenuLayer;
use crate::app::test_driver::{click_on, is_focused, press};
use crate::app::test_views::{self, Opener};

type Log = Rc<RefCell<Vec<&'static str>>>;

/// The menu's lines, by index: 0 header, 1 One, 2 Two, 3 separator, 4 Three (toggled), 5 Off
/// (disabled), 6 More (submenu: Inner, Other), 7 Last.
fn build(log: &Log, window: &mut Window, cx: &mut App) -> Entity<ContextMenu> {
    let log = log.clone();
    ContextMenu::build(window, cx, move |menu, _, _| {
        let entry = |label: &'static str| {
            let log = log.clone();
            move |_: &mut Window, _: &mut App| log.borrow_mut().push(label)
        };
        let inner_log = log.clone();
        menu.header("Head")
            .entry("One", None, entry("One"))
            .entry("Two", None, entry("Two"))
            .separator()
            .toggleable_entry("Three", true, IconPosition::Start, None, entry("Three"))
            .item(
                ContextMenuEntry::new("Off")
                    .disabled(true)
                    .handler(entry("Off")),
            )
            .submenu("More", move |menu, _, _| {
                let first = inner_log.clone();
                let second = inner_log.clone();
                menu.entry("Inner", None, move |_, _| first.borrow_mut().push("Inner"))
                    .entry("Other", None, move |_, _| second.borrow_mut().push("Other"))
            })
            .entry("Last", None, entry("Last"))
    })
}

struct Open<'a> {
    opener: Entity<Opener>,
    menu: Entity<ContextMenu>,
    log: Log,
    cx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Open<'_> {
    let (opener, _, cx) = test_views::init_opener(cx);
    let log: Log = Rc::default();
    let menu = cx.update(|window, cx| {
        let menu = build(&log, window, cx);
        MenuLayer::open(menu.clone(), point(px(40.), px(40.)), window, cx);
        menu
    });
    cx.run_until_parked();
    Open {
        opener,
        menu,
        log,
        cx,
    }
}

fn selected(menu: &Entity<ContextMenu>, cx: &mut VisualTestContext) -> Option<usize> {
    menu.read_with(cx, |menu, _| menu.selected_index())
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| MenuLayer::open_menu(cx)).is_some()
}

#[gpui::test]
fn t23_the_menu_takes_the_focus_with_the_toggled_entry_picked(cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    assert!(is_open(cx));
    assert!(is_focused(&open.menu, cx));
    assert!(!is_focused(&open.opener, cx));
    assert_eq!(selected(&open.menu, cx), Some(4));
}

#[gpui::test]
fn t23_the_moving_keys_pick_the_lines_they_pick_today(cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    let mut seen = Vec::new();
    for key in [
        "down", "down", "down", "down", "down", "down", "down", "up", "up", "home", "end",
        "pageup", "pagedown", "up",
    ] {
        press(cx, key);
        seen.push(format!("{key} {:?}", selected(&open.menu, cx)));
    }
    assert!(is_open(cx));
    assert!(open.log.borrow().is_empty());
    assert_eq!(
        seen,
        [
            "down Some(6)",
            "down Some(7)",
            "down Some(1)",
            "down Some(2)",
            "down Some(4)",
            "down Some(6)",
            "down Some(7)",
            "up Some(6)",
            "up Some(4)",
            "home Some(1)",
            "end Some(7)",
            "pageup Some(1)",
            "pagedown Some(7)",
            "up Some(6)",
        ]
    );
}

#[gpui::test]
fn t23_enter_runs_the_entry_once_and_gives_the_focus_back(cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    press(cx, "down down");
    // The second Enter comes after the menu closed, so it goes to the opener.
    press(cx, "enter enter");
    assert_eq!(*open.log.borrow(), ["Last"]);
    assert!(!is_open(cx));
    assert!(is_focused(&open.opener, cx));
}

#[gpui::test]
fn t23_space_runs_the_entry_as_enter_does(cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    press(cx, "down down down space");
    assert_eq!(*open.log.borrow(), ["One"]);
    assert!(!is_open(cx));
    assert!(is_focused(&open.opener, cx));
}

#[gpui::test]
fn t23_enter_at_once_runs_the_toggled_entry(cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    press(cx, "enter");
    assert_eq!(*open.log.borrow(), ["Three"]);
    assert!(!is_open(cx));
    assert!(is_focused(&open.opener, cx));
}

fn closes_with_nothing_run(key: &str, cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    press(cx, "down");
    press(cx, key);
    assert!(open.log.borrow().is_empty(), "{key}");
    assert!(!is_open(cx), "{key}");
    assert!(is_focused(&open.opener, cx), "{key}");
}

#[gpui::test]
fn t23_esc_closes_with_nothing_run(cx: &mut TestAppContext) {
    closes_with_nothing_run("escape", cx);
}

#[gpui::test]
fn t23_tab_closes_with_nothing_run(cx: &mut TestAppContext) {
    closes_with_nothing_run("tab", cx);
}

/// Right opens the submenu and moves the focus into it, outside the main menu, so the menu layer
/// no longer hands the keys over: Down, Enter, Left and Esc reach nothing and the menu stays.
#[gpui::test]
fn t23_right_opens_the_submenu_whose_keys_reach_nothing(cx: &mut TestAppContext) {
    let open = open(cx);
    let cx = open.cx;
    press(cx, "end up");
    assert_eq!(selected(&open.menu, cx), Some(6));
    let mut seen = Vec::new();
    for key in ["right", "down", "enter", "left", "escape"] {
        press(cx, key);
        let inside =
            cx.update(|window, cx| open.menu.focus_handle(cx).contains_focused(window, cx));
        seen.push(format!(
            "{key}: open {}, in main menu {inside}, picked {:?}, ran {:?}, opener {}",
            is_open(cx),
            selected(&open.menu, cx),
            open.log.borrow(),
            is_focused(&open.opener, cx)
        ));
    }
    assert_eq!(
        seen,
        [
            "right: open true, in main menu false, picked Some(6), ran [], opener false",
            "down: open true, in main menu false, picked Some(6), ran [], opener false",
            "enter: open true, in main menu false, picked Some(6), ran [], opener false",
            "left: open true, in main menu false, picked Some(6), ran [], opener false",
            "escape: open true, in main menu false, picked Some(6), ran [], opener false",
        ]
    );
}

#[gpui::test]
fn t23_a_disabled_entry_runs_nothing_and_keys_with_a_modifier_stay_with_the_app(
    cx: &mut TestAppContext,
) {
    let open = open(cx);
    let cx = open.cx;
    // Shift moves as the plain key does; Mod and Alt leave the menu as it is. A click on the
    // disabled entry runs nothing.
    open.menu.update_in(cx, |menu, window, cx| {
        menu.select_last(window, cx);
    });
    let mut seen = Vec::new();
    for key in ["shift-down", "mod-down", "alt-up"] {
        press(cx, key);
        seen.push(format!(
            "{key} {:?} open {}",
            selected(&open.menu, cx),
            is_open(cx)
        ));
    }
    click_on(cx, "MENU_ITEM-Off");
    assert!(open.log.borrow().is_empty());
    seen.push(format!("a click on Off: open {}", is_open(cx)));
    assert_eq!(
        seen,
        [
            "shift-down Some(1) open true",
            "mod-down Some(1) open true",
            "alt-up Some(1) open true",
            "a click on Off: open true",
        ]
    );
}
