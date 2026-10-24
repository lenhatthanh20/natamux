//! The items of the status bar (docs/ui-spec.md 4.28): the toggles at the left, the keyboard and
//! the account at the right.

use std::rc::Rc;

use gpui::{Action, ClickEvent, Entity};
use ui::{ButtonLike, Tooltip, prelude::*};

use super::account_menu::open_account_menu;
use super::commands::shortcut_of;
use super::dashboard::Dashboard;
use super::shortcut_text::keystroke;
use super::shortcuts::ShowShortcuts;
use crate::components::chips::tip_with_keys;
use crate::components::icons::icon_path;

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

/// A 24 px icon toggle that runs a command's action: the selected look while `on`. Disabled, it
/// shows `why` as its tooltip.
pub(super) fn toggle_item(
    id: &'static str,
    icon_name: &str,
    tip: &'static str,
    command: &'static str,
    on: bool,
    disabled_why: Option<&'static str>,
    action: fn() -> Box<dyn Action>,
) -> ButtonLike {
    let keys = shortcut_of(command);
    let button = ButtonLike::new(id)
        .height(rems_from_px(24.0_f32).into())
        .toggle_state(on)
        .on_click(move |_, window, cx| window.dispatch_action(action(), cx))
        .child(icon(icon_name));
    match disabled_why {
        Some(why) => button.disabled(true).tooltip(Tooltip::text(why)),
        None => button.tooltip(move |window, cx| match keys {
            Some(keys) => tip_with_keys(tip, keys)(window, cx),
            None => Tooltip::text(tip)(window, cx),
        }),
    }
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
