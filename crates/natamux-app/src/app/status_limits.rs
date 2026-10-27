//! The plan limits item of the status bar: the agent nearest a limit, with a small bar, and a
//! menu of every agent that told its limits (`agents/limits_rules.rs`).

use gpui::{ClickEvent, Div, Hsla, px};
use natamux_proto::agents::limits::AgentLimits;
use ui::{ButtonLike, ContextMenu, Tooltip, prelude::*};

use super::clock::now_ms;
use super::status_items::item_label;
use crate::agents::limits_rules::{Chip, Level, LimitRow, current, limit_rows};
use crate::components::menu::MenuLayer;

/// What the item and its menu show.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitsItem {
    pub chip: Chip,
    /// Made into rows when the menu opens, so its reset times are of that moment.
    pub limits: Vec<AgentLimits>,
    /// The agents in use that told no limits, by name.
    pub no_data: Vec<String>,
}

fn fill(level: Level, cx: &App) -> Hsla {
    let status = cx.theme().status();
    match level {
        Level::Normal => status.info,
        Level::Warn => status.warning,
        Level::Full => status.error,
    }
}

fn text_color(level: Level) -> Color {
    match level {
        Level::Normal => Color::Muted,
        Level::Warn => Color::Warning,
        Level::Full => Color::Error,
    }
}

/// A share of `width` filled in the level's colour.
fn share_bar(used: f64, width: f32, height: f32, level: Level, cx: &App) -> Div {
    let share = (used / 100.0).clamp(0.0, 1.0) as f32;
    div()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .rounded_full()
        .bg(cx.theme().colors().border_variant)
        .child(
            div()
                .h_full()
                .w(px(width * share))
                .rounded_full()
                .bg(fill(level, cx)),
        )
}

/// `narrow`: only the share, as under 1024.
pub fn limits_item(item: LimitsItem, narrow: bool, cx: &App) -> ButtonLike {
    let text = if narrow {
        item.chip.short.clone()
    } else {
        item.chip.text.clone()
    };
    let (used, level) = (item.chip.used, item.chip.level);
    ButtonLike::new("status-limits")
        .height(rems_from_px(24.0_f32).into())
        .tooltip(Tooltip::text("Show plan usage"))
        .on_click(move |event, window, cx| open_limits_menu(&item, event, window, cx))
        .child(
            h_flex()
                .gap(rems_from_px(6.0_f32))
                .child(share_bar(used, 24.0, 4.0, level, cx))
                .child(item_label(text, text_color(level))),
        )
}

fn row_element(row: &LimitRow, cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .gap_3()
        .child(
            div()
                .w(px(112.0))
                .child(Label::new(row.name.clone()).size(LabelSize::Small)),
        )
        .child(share_bar(row.used, 64.0, 6.0, row.level, cx))
        .child(
            div()
                .flex_1()
                .child(item_label(row.detail.clone(), text_color(row.level))),
        )
        .child(item_label(row.reset.clone(), Color::Muted))
        .into_any_element()
}

/// The menu, opened upward from the bar: one row per agent, the most used first.
fn open_limits_menu(item: &LimitsItem, event: &ClickEvent, window: &mut Window, cx: &mut App) {
    let now = now_ms();
    let rows = limit_rows(&current(&item.limits, now), now);
    let no_data = item.no_data.clone();
    let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
        menu = menu.header("Plan usage");
        for row in rows {
            menu = menu.custom_row(move |_: &mut Window, cx: &mut App| row_element(&row, cx));
        }
        if !no_data.is_empty() {
            let text = format!("No limit data: {}", no_data.join(", "));
            menu = menu
                .separator()
                .custom_row(move |_: &mut Window, _: &mut App| {
                    item_label(text.clone(), Color::Muted).into_any_element()
                });
        }
        menu
    });
    MenuLayer::open(menu, event.position(), window, cx);
}
