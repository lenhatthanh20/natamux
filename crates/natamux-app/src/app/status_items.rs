//! The items at the right of the status bar (docs/ui-spec.md 4.28): Projects, Import issues, the
//! keyboard and the account, and the "More status items" menu that holds the ones the width
//! folds away (`status_layout`).

use std::rc::Rc;

use gpui::{Action, ClickEvent, Entity};
use ui::{ButtonLike, ContextMenu, ContextMenuEntry, Tooltip, prelude::*};

use super::account_menu::open_account_menu;
use super::commands::shortcut_of;
use super::dashboard::Dashboard;
use super::shortcut_text::keystroke;
use super::shortcuts::{AddProject, ImportIssues, ShowShortcuts};
use crate::components::chips::tip_with_keys;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

pub(super) type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

const MAC: bool = cfg!(target_os = "macos");

/// A clickable item: 24 high, a muted label that turns to ink on hover.
pub(super) fn item_button(id: &'static str, tip: &'static str, on_click: OnClick) -> ButtonLike {
    ButtonLike::new(id)
        .height(rems_from_px(24.0_f32).into())
        .tooltip(Tooltip::text(tip))
        .on_click(move |event, window, cx| on_click(event, window, cx))
}

pub(super) fn item_label(text: impl Into<SharedString>, color: Color) -> Label {
    Label::new(text).size(LabelSize::Small).color(color)
}

fn icon(name: &str) -> Icon {
    Icon::from_path(icon_path(name))
        .size(IconSize::Small)
        .color(Color::Muted)
}

/// An item that runs an app command: an icon, and a label unless the width took it.
fn command_item(
    id: &'static str,
    tip: &'static str,
    icon_name: &str,
    label: &'static str,
    icon_only: bool,
    action: fn() -> Box<dyn Action>,
) -> ButtonLike {
    let run: OnClick = Rc::new(move |_, window, cx| window.dispatch_action(action(), cx));
    item_button(id, tip, run).child(
        h_flex()
            .gap(rems_from_px(6.0_f32))
            .child(icon(icon_name))
            .when(!icon_only, |this| {
                this.child(item_label(label, Color::Muted))
            }),
    )
}

pub(super) fn projects_item(icon_only: bool) -> ButtonLike {
    command_item(
        "status-projects",
        "Projects",
        "folder-git-2",
        "Projects",
        icon_only,
        || Box::new(AddProject),
    )
}

pub(super) fn import_item(icon_only: bool) -> ButtonLike {
    command_item(
        "status-import",
        "Import GitHub issues",
        "download",
        "Import issues",
        icon_only,
        || Box::new(ImportIssues),
    )
}

pub(super) fn keyboard_item() -> ButtonLike {
    let keys = shortcut_of("help.shortcuts")
        .map(|key| keystroke(key, MAC))
        .unwrap_or_default();
    ButtonLike::new("status-keyboard")
        .height(rems_from_px(24.0_f32).into())
        .tooltip(tip_with_keys("Keyboard shortcuts", &keys))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(ShowShortcuts), cx))
        .child(icon("keyboard"))
}

/// The account menu opens at the click: the menu moves up to stay inside the window.
pub(super) fn account_item(dashboard: Entity<Dashboard>) -> ButtonLike {
    ButtonLike::new("status-account")
        .height(rems_from_px(24.0_f32).into())
        .tooltip(Tooltip::text("Account"))
        .on_click(move |event, window, cx| {
            open_account_menu(&dashboard, event.position(), window, cx)
        })
        .child(icon("circle-user"))
}

/// What the folded ids of `status_layout` show in the menu, in the order of the bar.
pub(super) struct Folded {
    pub repos: bool,
    pub import: bool,
    pub notifications: Option<(bool, OnClick)>,
}

pub(super) fn more_item(folded: Folded) -> ButtonLike {
    ButtonLike::new("status-more")
        .height(rems_from_px(24.0_f32).into())
        .tooltip(Tooltip::text("More status items"))
        .on_click(move |event, window, cx| {
            let position = event.position();
            let event = event.clone();
            let (repos, import) = (folded.repos, folded.import);
            let notify = folded.notifications.clone();
            let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
                if repos {
                    menu = menu.item(entry("Projects", "folder-git-2", |window, cx| {
                        window.dispatch_action(Box::new(AddProject), cx)
                    }));
                }
                if import {
                    menu = menu.item(entry("Import issues", "download", |window, cx| {
                        window.dispatch_action(Box::new(ImportIssues), cx)
                    }));
                }
                if let Some((on, toggle)) = notify {
                    let (name, label) = if on {
                        ("bell-off", "Notifications on")
                    } else {
                        ("bell", "Notify me")
                    };
                    menu = menu.item(entry(label, name, move |window, cx| {
                        toggle(&event, window, cx)
                    }));
                }
                menu
            });
            MenuLayer::open(menu, position, window, cx);
        })
        .child(icon("ellipsis"))
}

fn entry(
    label: &'static str,
    icon_name: &str,
    run: impl Fn(&mut Window, &mut App) + 'static,
) -> ContextMenuEntry {
    ContextMenuEntry::new(label)
        .custom_icon_path(icon_path(icon_name))
        .icon_color(Color::Muted)
        .handler(run)
}
