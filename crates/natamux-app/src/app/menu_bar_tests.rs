//! The menu bar of Windows and Linux (T-0364): shown off macOS or when forced, the same rows as
//! the macOS menus, a hover moves to the next menu, a second click closes, and an item reaches
//! the view that had the focus.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{
    Entity, Focusable, MenuItem, Modifiers, Pixels, Point, SharedString, TestAppContext,
    VisualTestContext, point, px,
};
use ui::ContextMenu;

use super::{entry, force_menu_row, menu_row_shown, open_app_menu, open_menu_name};
use crate::app::command_types::MenuName;
use crate::app::dashboard::Dashboard;
use crate::app::shortcuts::{ShowAgents, menu_of};
use crate::app::test_driver::{click_on, press};
use crate::app::window::flow_tests::{parts, root};
use crate::components::menu::MenuLayer;

/// Under the title bar, at the left.
fn at() -> Point<Pixels> {
    point(px(8.), px(34.))
}

/// The rows the macOS menu has: `-` for a separator, else the item's title.
fn os_rows(name: MenuName) -> Vec<SharedString> {
    menu_of(name)
        .into_iter()
        .filter_map(|item| match item {
            MenuItem::Separator => Some("-".into()),
            MenuItem::Action { name, .. } => Some(name),
            _ => None,
        })
        .collect()
}

/// The (disabled, ticked) flags the macOS menu has, row by row.
fn os_states(name: MenuName) -> Vec<(bool, bool)> {
    menu_of(name)
        .into_iter()
        .filter_map(|item| match item {
            MenuItem::Separator => Some((false, false)),
            MenuItem::Action {
                checked, disabled, ..
            } => Some((disabled, checked)),
            _ => None,
        })
        .collect()
}

fn open_rows(cx: &mut VisualTestContext) -> Option<Vec<SharedString>> {
    let menu = cx.update(|_, cx| MenuLayer::open_menu(cx))?;
    Some(menu.read_with(cx, |menu, _| menu.row_texts()))
}

fn open_states(cx: &mut VisualTestContext) -> Option<Vec<(bool, bool)>> {
    let menu = cx.update(|_, cx| MenuLayer::open_menu(cx))?;
    Some(menu.read_with(cx, |menu, _| menu.row_states()))
}

fn bar_button(name: MenuName) -> &'static str {
    match name {
        MenuName::App => "MENU_BAR-Natamux",
        MenuName::View => "MENU_BAR-View",
        MenuName::Task => "MENU_BAR-Task",
        MenuName::Tools => "MENU_BAR-Tools",
        MenuName::Help => "MENU_BAR-Help",
    }
}

#[gpui::test]
fn the_row_shows_off_macos_or_when_a_shot_forces_it(cx: &mut TestAppContext) {
    cx.update(|cx| {
        assert_eq!(menu_row_shown(cx), cfg!(not(target_os = "macos")));
        force_menu_row(cx);
        assert!(menu_row_shown(cx));
    });
}

#[gpui::test]
fn each_menu_has_the_rows_of_the_os_menu(cx: &mut TestAppContext) {
    let (_root, _dashboard, cx) = root(cx);
    for name in MenuName::ALL {
        cx.update(|window, cx| open_app_menu(name, at(), window, cx));
        cx.run_until_parked();
        assert_eq!(open_rows(cx), Some(os_rows(name)), "{name:?}");
        assert_eq!(open_states(cx), Some(os_states(name)), "{name:?}");
        assert_eq!(cx.update(|_, cx| open_menu_name(cx)), Some(name));
        press(cx, "escape");
        assert_eq!(cx.update(|_, cx| open_menu_name(cx)), None);
    }
}

#[gpui::test]
fn a_click_opens_under_the_button_and_a_second_click_closes(cx: &mut TestAppContext) {
    let (_root, _dashboard, cx) = root(cx);
    cx.update(|_, cx| force_menu_row(cx));
    cx.run_until_parked();
    click_on(cx, bar_button(MenuName::View));
    cx.run_until_parked();
    let first = cx.update(|_, cx| open_menu_name(cx));
    click_on(cx, bar_button(MenuName::View));
    cx.run_until_parked();
    let second = cx.update(|_, cx| open_menu_name(cx));
    assert_eq!((first, second), (Some(MenuName::View), None));
}

#[gpui::test]
fn a_hover_moves_to_the_next_menu_only_while_one_is_open(cx: &mut TestAppContext) {
    let (_root, _dashboard, cx) = root(cx);
    cx.update(|_, cx| force_menu_row(cx));
    cx.run_until_parked();
    let task = cx
        .debug_bounds(bar_button(MenuName::Task))
        .expect("the Task button is drawn");
    // Nothing open: a hover opens nothing.
    cx.simulate_mouse_move(task.center(), None, Modifiers::none());
    cx.run_until_parked();
    let before = cx.update(|_, cx| open_menu_name(cx));
    click_on(cx, bar_button(MenuName::View));
    cx.run_until_parked();
    cx.simulate_mouse_move(task.center(), None, Modifiers::none());
    cx.run_until_parked();
    let after = cx.update(|_, cx| open_menu_name(cx));
    let focused = cx.update(|window, cx| {
        MenuLayer::open_menu(cx).is_some_and(|menu| menu.focus_handle(cx).is_focused(window))
    });
    assert_eq!((before, after, focused), (None, Some(MenuName::Task), true));
}

/// Enter on an item sends its action to the view that had the focus, as Zed's menu does, so the
/// root view gets it (the add menu's Enter does not, see `window_menu_tests`).
#[gpui::test]
fn enter_on_an_item_reaches_the_root_view(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let tasks = parts(&root, cx).tasks;
    let handle = tasks.read_with(cx, |tasks, cx| tasks.focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
    cx.update(|window, cx| open_app_menu(MenuName::View, at(), window, cx));
    cx.run_until_parked();
    // The View menu starts with All tasks, then Agents.
    press(cx, "down");
    press(cx, "enter");
    cx.run_until_parked();
    let agents = dashboard.read_with(cx, |dashboard, _| dashboard.agents_open);
    let open = cx.update(|_, cx| open_menu_name(cx));
    assert_eq!((agents, open), (true, None));
}

/// The flags of a menu item reach its entry, whatever the update state is now.
#[gpui::test]
fn an_entry_keeps_the_disabled_and_ticked_flags(cx: &mut TestAppContext) {
    let (_root, _dashboard, cx) = root(cx);
    let states = cx.update(|window, cx| {
        let menu = ContextMenu::build(window, cx, |menu, _, _| {
            let flags = [(false, false), (true, false), (false, true), (true, true)];
            flags.into_iter().fold(menu, |menu, (checked, disabled)| {
                let label = format!("{checked} {disabled}");
                menu.item(entry(
                    label.into(),
                    Box::new(ShowAgents),
                    checked,
                    disabled,
                    None,
                ))
            })
        });
        menu.read(cx).row_states()
    });
    // (disabled, ticked) of each row.
    assert_eq!(
        states,
        [(false, false), (false, true), (true, false), (true, true)]
    );
}

/// Opens one Agents entry, picks it by Enter and by a click, and tells whether the grid opened.
fn pick_agents(
    disabled: bool,
    dashboard: &Entity<Dashboard>,
    cx: &mut VisualTestContext,
) -> (bool, bool) {
    let mut opened = Vec::new();
    for by_click in [false, true] {
        cx.update(|window, cx| {
            let context = window.focused(cx);
            let menu = ContextMenu::build(window, cx, move |menu, _, _| {
                menu.item(entry(
                    "Agents".into(),
                    Box::new(ShowAgents),
                    false,
                    disabled,
                    context,
                ))
            });
            MenuLayer::open(menu, at(), window, cx);
        });
        cx.run_until_parked();
        if by_click {
            click_on(cx, "MENU_ITEM-Agents");
        } else {
            press(cx, "enter");
        }
        cx.run_until_parked();
        opened.push(dashboard.read_with(cx, |dashboard, _| dashboard.agents_open));
        press(cx, "escape");
        dashboard.update(cx, |dashboard, cx| {
            dashboard.agents_open = false;
            cx.notify();
        });
        cx.run_until_parked();
    }
    (opened[0], opened[1])
}

#[gpui::test]
fn a_disabled_item_does_nothing(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let tasks = parts(&root, cx).tasks;
    let handle = tasks.read_with(cx, |tasks, cx| tasks.focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
    let disabled = pick_agents(true, &dashboard, cx);
    let enabled = pick_agents(false, &dashboard, cx);
    assert_eq!((disabled, enabled), ((false, false), (true, true)));
}
