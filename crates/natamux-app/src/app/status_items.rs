//! The items of the status bar (docs/ui-spec.md 4.28): the toggles and the counts.

use std::rc::Rc;

use gpui::{Action, ClickEvent};
use ui::{ButtonLike, Tooltip, prelude::*};

use super::commands::shortcut_of;
use crate::components::chips::tip_with_keys;
use crate::components::icons::icon_path;

pub(super) type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A count as Zed's diagnostics: 24 high, an icon and the number; the words are in `tip`.
pub(super) fn count_item(
    id: &'static str,
    icon_name: &str,
    color: Color,
    count: Label,
    tip: String,
) -> ButtonLike {
    ButtonLike::new(id)
        .height(rems_from_px(24.0_f32).into())
        .tooltip(Tooltip::text(tip))
        .child(
            h_flex()
                .gap(rems_from_px(4.0_f32))
                .child(
                    Icon::from_path(icon_path(icon_name))
                        .size(IconSize::Small)
                        .color(color),
                )
                .child(count),
        )
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
