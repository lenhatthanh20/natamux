//! The Theme rows of the Appearance page: the preview tiles of System, Light and Dark, and the
//! Light theme and Dark theme selects, which open a menu of the themes the app offers for that
//! look (`theme_catalog.rs`).

use std::rc::Rc;

use gpui::{
    ClickEvent, Entity, FocusHandle, Hsla, KeyDownEvent, Pixels, Point, Stateful, relative,
};
use ui::{ContextMenu, prelude::*};

use super::appearance_rules::{ThemeChoice, parse_theme};
use super::dashboard::Dashboard;
use super::theme_catalog::ThemeKind;
use crate::components::colors::accent;
use crate::components::icons::icon_path;
use crate::components::input_rules::segment_step;
use crate::components::menu::MenuLayer;
use crate::stages::stage_fields_view::{SELECT_WIDTH, select};

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
    let button =
        select(id, current)
            .track_focus(focus)
            .on_click(move |event: &ClickEvent, window, cx| {
                // At the pointer for a mouse click, under the button for Enter or Space.
                open_menu(kind, &dashboard, event.position(), window, cx)
            });
    div()
        .flex_none()
        .w(rems_from_px(SELECT_WIDTH))
        .child(button)
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

/// The Theme row's control: a preview tile per choice (System, Light, Dark). One tab stop; Left
/// and Right pick, as in a radio group.
pub fn theme_tiles(focus: &FocusHandle, dashboard: &Entity<Dashboard>, cx: &App) -> AnyElement {
    let theme = parse_theme(&dashboard.read(cx).view().theme);
    let selected = ThemeChoice::ALL
        .iter()
        .position(|choice| *choice == theme)
        .unwrap_or(0);
    let pick = {
        let dashboard = dashboard.clone();
        Rc::new(move |index: usize, cx: &mut App| {
            let Some(choice) = ThemeChoice::ALL.get(index) else {
                return;
            };
            let name = choice.name();
            dashboard.update(cx, |dashboard, cx| dashboard.set_theme(name, cx))
        })
    };
    let light = preview_colors(ThemeKind::Light, cx);
    let dark = preview_colors(ThemeKind::Dark, cx);
    let tiles = ThemeChoice::ALL.iter().enumerate().map(|(index, choice)| {
        let panels = match choice {
            ThemeChoice::System => vec![light, dark],
            ThemeChoice::Light => vec![light],
            ThemeChoice::Dark => vec![dark],
        };
        let pick = pick.clone();
        theme_tile(*choice, index == selected, panels, cx).on_click(move |_, _, cx| pick(index, cx))
    });
    let by_key = pick.clone();
    h_flex()
        .id("settings-theme")
        .flex_none()
        .gap(rems_from_px(12.0_f32))
        .track_focus(focus)
        .on_key_down(move |event: &KeyDownEvent, _, cx| {
            if let Some(next) = segment_step(ThemeChoice::ALL.len(), selected, &event.keystroke.key)
            {
                if next != selected {
                    by_key(next, cx);
                }
                cx.stop_propagation();
            }
        })
        .children(tiles)
        .into_any_element()
}

/// (background, bar, accent) of a theme of the app, read from the theme itself so no colour is
/// written here. The accent bar is Natamux Dark's in both, as in the mockup.
fn preview_colors(kind: ThemeKind, cx: &App) -> (Hsla, Hsla, Hsla) {
    let registry = theme::ThemeRegistry::global(cx);
    let bar_accent = registry
        .get(ThemeKind::Dark.default_theme())
        .map_or_else(|_| accent(cx), |theme| theme.players().local().cursor);
    match registry.get(kind.default_theme()) {
        Ok(theme) => (
            theme.colors().editor_background,
            theme.colors().border,
            bar_accent,
        ),
        Err(_) => {
            let colors = cx.theme().colors();
            (colors.editor_background, colors.border, bar_accent)
        }
    }
}

fn theme_tile(
    choice: ThemeChoice,
    on: bool,
    panels: Vec<(Hsla, Hsla, Hsla)>,
    cx: &App,
) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let bar = |width: f32, color: Hsla| {
        div()
            .h(rems_from_px(4.0_f32))
            .w(relative(width))
            .rounded(rems_from_px(2.0_f32))
            .bg(color)
    };
    let minis = panels.into_iter().map(|(bg, fg, accent)| {
        v_flex()
            .flex_1()
            .h_full()
            .gap(rems_from_px(3.0_f32))
            .p(rems_from_px(6.0_f32))
            .bg(bg)
            .child(bar(0.6, fg))
            .child(bar(0.8, fg.opacity(0.6)))
            .child(bar(0.4, accent))
    });
    let preview = h_flex()
        .w(rems_from_px(84.0_f32))
        .h(rems_from_px(52.0_f32))
        .rounded(rems_from_px(6.0_f32))
        .overflow_hidden()
        .map(|preview| {
            if on {
                preview.border_2().border_color(accent(cx))
            } else {
                preview.border_1().border_color(colors.border).p(px(1.))
            }
        })
        .children(minis);
    let color = if on { Color::Default } else { Color::Muted };
    v_flex()
        .id(SharedString::from(format!(
            "settings-theme-{}",
            choice.name()
        )))
        .items_center()
        .gap(rems_from_px(4.0_f32))
        .child(preview)
        .child(
            h_flex()
                .gap(rems_from_px(4.0_f32))
                .child(
                    Icon::from_path(icon_path(caption_icon(choice)))
                        .size(IconSize::Custom(rems_from_px(11.0_f32)))
                        .color(color),
                )
                .child(
                    Label::new(choice.label())
                        .size(LabelSize::Small)
                        .color(color),
                ),
        )
}

/// The Lucide icon before a theme tile's caption (layout-b 4.2).
fn caption_icon(choice: ThemeChoice) -> &'static str {
    match choice {
        ThemeChoice::System => "monitor",
        ThemeChoice::Light => "sun",
        ThemeChoice::Dark => "moon",
    }
}
