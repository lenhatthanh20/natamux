//! The Light theme and Dark theme rows of the Appearance page: a select with the theme of that
//! look, which opens a menu of the themes the app offers for it (`theme_catalog.rs`).

use gpui::{ClickEvent, Entity, FocusHandle, Pixels, Point};
use ui::{ContextMenu, prelude::*};

use super::dashboard::Dashboard;
use super::theme_catalog::ThemeKind;
use crate::components::menu::MenuLayer;

pub fn theme_select(
    kind: ThemeKind,
    focus: &FocusHandle,
    dashboard: &Entity<Dashboard>,
    cx: &App,
) -> AnyElement {
    let id = match kind {
        ThemeKind::Light => "settings-light-theme",
        ThemeKind::Dark => "settings-dark-theme",
    };
    let current = dashboard.read(cx).view().themes.get(kind);
    let dashboard = dashboard.clone();
    Button::new(id, current)
        .style(ButtonStyle::Outlined)
        .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small))
        .track_focus(focus)
        .on_click(move |event: &ClickEvent, window, cx| {
            // At the pointer for a mouse click, under the button for Enter or Space.
            open_menu(kind, &dashboard, event.position(), window, cx)
        })
        .into_any_element()
}

fn open_menu(
    kind: ThemeKind,
    dashboard: &Entity<Dashboard>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let current = dashboard.read(cx).view().themes.get(kind);
    let dashboard = dashboard.clone();
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        kind.themes().iter().fold(menu, |menu, &name| {
            let dashboard = dashboard.clone();
            let checked = name == current;
            menu.toggleable_entry(name, checked, IconPosition::Start, None, move |_, cx| {
                dashboard.update(cx, |dashboard, cx| dashboard.set_theme_of(kind, name, cx))
            })
        })
    });
    MenuLayer::open(menu, position, window, cx);
}
