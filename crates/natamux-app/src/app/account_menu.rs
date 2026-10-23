//! The account menu (docs/ui-spec.md 4.13, `AccountMenu.tsx`): settings, theme, UI size, terminal
//! font, the shortcut sheet and the issue page. There is no sign out in the native app.

use gpui::{Entity, Pixels, Point};
use ui::{ContextMenu, ContextMenuEntry, IconPosition, prelude::*};

use super::appearance_rules::{ThemeChoice, UiSize, parse_theme, parse_ui_size};
use super::commands::shortcut_of;
use super::dashboard::Dashboard;
use super::shortcut_text::keystroke;
use super::shortcuts::{OpenSettings, ShowShortcuts};
use crate::components::chips::Kbd;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;
use crate::terminals::terminal_font_dialog;

const MAC: bool = cfg!(target_os = "macos");
const ISSUES_URL: &str = "https://github.com/lenhatthanh20/natamux/issues/new";

/// A row with an icon, a label and the key chip of command `id`. Written by hand because the
/// menu's own key column needs the key bound where the menu was opened.
fn keyed_entry(
    menu: ContextMenu,
    icon: &'static str,
    label: &'static str,
    id: &'static str,
    run: impl Fn(&mut Window, &mut App) + 'static,
) -> ContextMenu {
    let keys = shortcut_of(id).map(|key| keystroke(key, MAC));
    menu.custom_entry(
        move |_, _| {
            h_flex()
                .w_full()
                .gap_2()
                .child(
                    Icon::from_path(icon_path(icon))
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .child(div().flex_1().child(Label::new(label)))
                .children(keys.as_deref().map(Kbd::new))
                .into_any_element()
        },
        run,
    )
}

/// Opens the menu with its top left corner at `position`; near the window's edge it moves in.
pub fn open_account_menu(
    dashboard: &Entity<Dashboard>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let view = dashboard.read(cx).view();
    let (theme, size) = (parse_theme(&view.theme), parse_ui_size(&view.ui_size));
    let dashboard = dashboard.clone();
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        menu = keyed_entry(
            menu,
            "settings",
            "Settings…",
            "app.settings",
            |window, cx| window.dispatch_action(Box::new(OpenSettings), cx),
        );
        menu = menu.separator().header("Theme");
        for choice in ThemeChoice::ALL {
            let set = dashboard.clone();
            menu = menu.toggleable_entry(
                choice.label(),
                choice == theme,
                IconPosition::Start,
                None,
                move |_, cx| set.update(cx, |d, cx| d.set_theme(choice.name(), cx)),
            );
        }
        menu = menu.separator().header("UI size");
        for choice in UiSize::ALL {
            let set = dashboard.clone();
            menu = menu.toggleable_entry(
                choice.label(),
                choice == size,
                IconPosition::Start,
                None,
                move |_, cx| set.update(cx, |d, cx| d.set_ui_size(choice.name(), cx)),
            );
        }
        let font = dashboard.clone();
        menu = menu.item(
            ContextMenuEntry::new("Terminal font…")
                .icon(IconName::FontSize)
                .icon_color(Color::Muted)
                .handler(move |window, cx| terminal_font_dialog::open(&font, window, cx)),
        );
        menu = menu.separator();
        menu = keyed_entry(
            menu,
            "keyboard",
            "Keyboard shortcuts",
            "help.shortcuts",
            |window, cx| window.dispatch_action(Box::new(ShowShortcuts), cx),
        );
        menu.item(
            ContextMenuEntry::new("Report an issue")
                .icon(IconName::Debug)
                .icon_color(Color::Muted)
                .handler(|_, cx| cx.open_url(ISSUES_URL)),
        )
    });
    MenuLayer::open(menu, position, window, cx);
}
