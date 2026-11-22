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
use crate::app::settings_rules::SettingsFocus;
use crate::app::shortcuts::{ShowAgents, ShowWhatsNew, menu_of};
use crate::app::test_driver::{click_on, press};
use crate::app::window::flow_tests::{REVIEWED, parts, root};
use crate::components::dialog::DialogLayer;
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

/// T-0368: the opener's focus is no longer drawn (a closed Settings box). The action then
/// started at the window's root node, above every handler, and Help items did nothing.
#[gpui::test]
fn an_item_reaches_the_root_view_when_the_opener_focus_is_gone(cx: &mut TestAppContext) {
    let (_root, _dashboard, cx) = root(cx);
    let gone = cx.update(|window, cx| {
        let gone = cx.focus_handle();
        window.focus(&gone, cx);
        gone
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            menu.item(entry(
                "What's New".into(),
                Box::new(ShowWhatsNew),
                false,
                false,
                Some(gone),
            ))
        });
        MenuLayer::open(menu, at(), window, cx);
    });
    cx.run_until_parked();
    click_on(cx, "MENU_ITEM-What's New");
    cx.run_until_parked();
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
}

/// T-0368: opening a task or Agents over Settings (the jump palette, a toast) closed Settings
/// but left the focus on its search box, which is no longer drawn.
#[gpui::test]
fn opening_a_task_or_agents_over_settings_moves_the_focus_out(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let root_focus = root.read_with(cx, |root, cx| root.focus_handle(cx));
    for agents in [false, true] {
        dashboard.update(cx, |dashboard, cx| {
            dashboard.open_settings(None, SettingsFocus::Search, cx)
        });
        cx.run_until_parked();
        let in_settings = cx.update(|window, _| !root_focus.is_focused(window));
        dashboard.update(cx, |dashboard, cx| {
            if agents {
                dashboard.open_agents(cx);
            } else {
                dashboard.open(REVIEWED, cx);
            }
        });
        cx.run_until_parked();
        let closed = dashboard.read_with(cx, |dashboard, _| dashboard.settings.is_none());
        let drawn = cx.update(|window, cx| root_focus.contains_focused(window, cx));
        assert_eq!(
            (in_settings, closed, drawn),
            (true, true, true),
            "agents: {agents}"
        );
    }
}

/// Beside the menu buttons the view name ("All tasks", "Agents") is not shown; without the
/// row (macOS) it is.
#[gpui::test]
fn the_view_name_hides_beside_the_menu_row(cx: &mut TestAppContext) {
    let (_root, dashboard, cx) = root(cx);
    dashboard.update(cx, |dashboard, cx| dashboard.back(cx));
    cx.run_until_parked();
    let shown = cx.debug_bounds("TITLE_VIEW_NAME").is_some();
    let row = cx.update(|_, cx| menu_row_shown(cx));
    cx.update(|_, cx| force_menu_row(cx));
    cx.run_until_parked();
    let forced = cx.debug_bounds("TITLE_VIEW_NAME").is_some();
    assert_eq!((shown, forced), (!row, false));
}

/// Every enabled menu item reaches a handler from Root's focus, in the list, a task and
/// Agents, with and without Settings used first. An item handled only in a child view would
/// do nothing from the menu bar (T-0368). Items disabled by design are skipped.
#[gpui::test]
fn every_menu_item_is_available_from_the_root_focus(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let root_focus = root.read_with(cx, |root, cx| root.focus_handle(cx));
    let mut missing = Vec::new();
    for settings_first in [false, true] {
        for state in ["list", "task", "agents"] {
            dashboard.update(cx, |dashboard, cx| dashboard.back(cx));
            cx.run_until_parked();
            if settings_first {
                dashboard.update(cx, |dashboard, cx| {
                    dashboard.open_settings(None, SettingsFocus::Search, cx)
                });
                cx.run_until_parked();
            }
            dashboard.update(cx, |dashboard, cx| match state {
                "task" => dashboard.open(REVIEWED, cx),
                "agents" => dashboard.open_agents(cx),
                _ => dashboard.close_settings(cx),
            });
            cx.run_until_parked();
            for name in MenuName::ALL {
                for item in menu_of(name) {
                    let MenuItem::Action {
                        name: title,
                        action,
                        disabled: false,
                        ..
                    } = item
                    else {
                        continue;
                    };
                    let reaches = cx.update(|window, _| {
                        window.is_action_available_in(action.as_ref(), &root_focus)
                    });
                    if !reaches {
                        missing.push(format!("{state} settings:{settings_first} {title}"));
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}
