//! The app's pages of Settings (layout-b 4.1, 4.2, 4.6): General and Appearance as groups of
//! cards, and the search results, whose rows are the same rows with working controls.

use std::rc::Rc;

use gpui::{ClickEvent, FocusHandle, KeyDownEvent};
use ui::{Switch, Tooltip, prelude::*};

use super::super::appearance_rules::{UiSize, parse_ui_size};
use super::super::dashboard::Dashboard;
use super::super::settings_rules::{
    SearchResults, SettingRow, SettingsPage, no_match_text, page_groups, results_groups,
};
use super::super::settings_theme::{theme_select, theme_tiles};
use super::super::shortcuts::RestartToUpdate;
use super::super::theme_catalog::ThemeKind;
use super::super::update;
use super::super::update_state::UpdateState;
use super::{PAGE_MAX, SettingsView, segment};
use crate::components::button_style::{action_button, button_icon};
use crate::components::colors::TileColor;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, SettingsRow};
use crate::components::settings_tile::{SettingsTile, TileSize};

const UPDATES_FOOT: &str = "Natamux applies app settings at once. Nothing to save.";
const PROJECT_HELP: &str = "Stages, agents, Git and issues";

/// A 24 px tile at the start of a row.
fn row_tile(icon: &str, color: TileColor) -> SettingsTile {
    SettingsTile::icon(button_icon(icon), color, TileSize::Row)
}

/// The body of an app page or of the results: it scrolls, and its content stays 760 wide and
/// centred on a wide window.
pub(super) fn scroll_body(id: &'static str, inner: impl IntoElement) -> AnyElement {
    v_flex()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .pt(rems_from_px(8.0_f32))
        .px(rems_from_px(32.0_f32))
        .pb(rems_from_px(32.0_f32))
        .child(
            h_flex()
                .w_full()
                .justify_center()
                .child(v_flex().w_full().max_w(rems_from_px(PAGE_MAX)).child(inner)),
        )
        .into_any_element()
}

impl SettingsView {
    /// A row with the label and help of `row`; the caller adds the controls.
    fn plain_row(row: SettingRow) -> SettingsRow {
        SettingsRow::new(
            SharedString::from(format!("settings-row-{row:?}")),
            row.label(),
        )
        .help(row.description())
    }

    fn row(&mut self, row: SettingRow, cx: &mut Context<Self>) -> AnyElement {
        let drawn = match row {
            SettingRow::Notify => Self::plain_row(row)
                .lead(row_tile("bell", TileColor::Red))
                .control(self.notify_switch(cx)),
            SettingRow::NotifyAlways => Self::plain_row(row).control(self.notify_always_switch(cx)),
            SettingRow::UpdatesVersion => self.version_row(),
            SettingRow::UpdatesAuto => Self::plain_row(row).control(self.updates_switch(cx)),
            SettingRow::Theme => Self::plain_row(row).spacing(0., 12.).control(theme_tiles(
                &self.theme_focus,
                &self.dashboard,
                cx,
            )),
            SettingRow::LightTheme => Self::plain_row(row).control(theme_select(
                ThemeKind::Light,
                &self.light_focus,
                &self.dashboard,
                cx,
            )),
            SettingRow::DarkTheme => Self::plain_row(row).control(theme_select(
                ThemeKind::Dark,
                &self.dark_focus,
                &self.dashboard,
                cx,
            )),
            SettingRow::UiSize => Self::plain_row(row).control(self.size_control(cx)),
            SettingRow::SidebarIds => Self::plain_row(row).control(self.sidebar_ids_switch(cx)),
            SettingRow::FontFamily => Self::plain_row(row).control(self.font.family_control(cx)),
            SettingRow::FontSize => Self::plain_row(row)
                .error(self.font.size_error(cx))
                .control(self.font.size_control(cx)),
            SettingRow::CustomAgents => Self::plain_row(row).control(
                action_button("settings-open-agents", "Open").on_click(cx.listener(
                    |view, _: &ClickEvent, window, cx| {
                        view.open_page(SettingsPage::Agents, window, cx)
                    },
                )),
            ),
        };
        drawn.into_any_element()
    }

    /// "Natamux <version>", the updater's state under it, and Check now (or Restart to Update).
    fn version_row(&self) -> SettingsRow {
        let state = update::state();
        let ago = update::checked_ago();
        let line = state.settings_line(update::bytes(), ago.as_deref());
        let color = match &state {
            UpdateState::Ready { .. } => Color::Success,
            UpdateState::Failed { .. } => Color::Error,
            _ => Color::Muted,
        };
        SettingsRow::new(
            "settings-row-version",
            format!("Natamux {}", update::running_version()),
        )
        .lead(row_tile("download", TileColor::Blue))
        .help_child(Label::new(line).size(LabelSize::Small).color(color))
        .control(self.check_button())
    }

    /// Check now, or Restart to Update while an update is ready; off while a check runs.
    fn check_button(&self) -> AnyElement {
        let state = update::state();
        let (label, enabled) = state.check_button();
        let ready = matches!(&state, UpdateState::Ready { .. });
        action_button("settings-update-check", label)
            .disabled(!enabled)
            .tooltip(Tooltip::text("Look up a new version now."))
            .track_focus(&self.check_focus)
            .on_click(move |_, window, cx| {
                if ready {
                    window.dispatch_action(Box::new(RestartToUpdate), cx);
                } else {
                    update::check(true, cx);
                }
            })
            .into_any_element()
    }

    fn notify_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.dashboard.read(cx).view().notify;
        self.view_switch(
            "settings-notify",
            &self.notify_focus,
            on,
            Dashboard::toggle_notify,
        )
    }

    fn notify_always_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.dashboard.read(cx).view().notify_always;
        self.view_switch(
            "settings-notify-always",
            &self.notify_always_focus,
            on,
            Dashboard::toggle_notify_always,
        )
    }

    fn sidebar_ids_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.dashboard.read(cx).view().sidebar_ids;
        self.view_switch(
            "settings-sidebar-ids",
            &self.sidebar_ids_focus,
            on,
            Dashboard::toggle_sidebar_ids,
        )
    }

    /// A switch of a `view.json` flag: a click, or Space or Enter on its stop, flips it.
    fn view_switch(
        &self,
        id: &'static str,
        focus: &FocusHandle,
        on: bool,
        toggle: fn(&mut Dashboard, &mut Context<Dashboard>),
    ) -> AnyElement {
        let dashboard = self.dashboard.clone();
        let flip = Rc::new(move |cx: &mut App| dashboard.update(cx, toggle));
        let by_key = flip.clone();
        div()
            .id(SharedString::from(format!("{id}-stop")))
            .track_focus(focus)
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    by_key(cx);
                }
            })
            .child(Switch::new(id, on.into()).on_click(move |_, _, cx| flip(cx)))
            .into_any_element()
    }

    /// The updater keeps this switch in its own file, not in `view.json`.
    fn updates_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("settings-update-stop")
            .track_focus(&self.update_focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    update::toggle_automatic(cx);
                    cx.notify();
                }
            }))
            .child(
                Switch::new("settings-update-auto", update::automatic().into()).on_click(
                    cx.listener(|_, _, _, cx| {
                        update::toggle_automatic(cx);
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }

    fn size_control(&self, cx: &App) -> AnyElement {
        let size = parse_ui_size(&self.dashboard.read(cx).view().ui_size);
        let selected = UiSize::ALL
            .iter()
            .position(|choice| *choice == size)
            .unwrap_or(0);
        let dashboard = self.dashboard.clone();
        segment(
            "settings-ui-size",
            &self.size_focus,
            UiSize::ALL.map(UiSize::label),
            selected,
            move |index, _, cx| {
                let Some(choice) = UiSize::ALL.get(index) else {
                    return;
                };
                let name = choice.name();
                dashboard.update(cx, |dashboard, cx| dashboard.set_ui_size(name, cx))
            },
        )
        .into_any_element()
    }

    /// General or Appearance: one card per group, with its footnote.
    pub(super) fn app_page(&mut self, page: &SettingsPage, cx: &mut Context<Self>) -> AnyElement {
        let mut groups: Vec<AnyElement> = Vec::new();
        for (title, rows) in page_groups(page) {
            let font = rows.contains(&SettingRow::FontFamily);
            let mut drawn: Vec<AnyElement> =
                rows.into_iter().map(|row| self.row(row, cx)).collect();
            if font {
                drawn.push(self.font.preview(cx));
            }
            let mut group = SettingsGroup::new(title)
                .child(SettingsCard::new(SeparatorInset::Text).children(drawn));
            if title == SettingRow::UpdatesAuto.group() {
                group = group.footnote(UPDATES_FOOT);
            }
            if font {
                let (text, reset) = self.font.footnote(cx);
                group = group.footnote(text);
                if let Some(reset) = reset {
                    group = group.footnote_child(reset);
                }
            }
            groups.push(group.into_any_element());
        }
        let id = match page {
            SettingsPage::General => "settings-general",
            _ => "settings-appearance",
        };
        scroll_body(id, v_flex().children(groups))
    }

    /// The rows a search found, under their page, then the projects it found.
    pub(super) fn results(
        &mut self,
        found: &SearchResults,
        query: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if found.is_empty() {
            return v_flex()
                .flex_none()
                .w_full()
                .h(rems_from_px(300.0_f32))
                .items_center()
                .justify_center()
                .child(Label::new(no_match_text(query)).color(Color::Muted))
                .into_any_element();
        }
        let mut parts: Vec<AnyElement> = Vec::new();
        for (_, title, rows) in results_groups(found) {
            let drawn: Vec<AnyElement> = rows.into_iter().map(|row| self.row(row, cx)).collect();
            let card = SettingsCard::new(SeparatorInset::Text).children(drawn);
            parts.push(SettingsGroup::new(title).child(card).into_any_element());
        }
        if !found.projects.is_empty() {
            let view = cx.entity();
            let rows = found
                .projects
                .iter()
                .enumerate()
                .map(|(index, (id, name))| {
                    let (view, page) = (view.clone(), SettingsPage::Project(id.clone()));
                    SettingsRow::new(("settings-result-project", index), name.clone())
                        .lead(SettingsTile::avatar(name, TileSize::Row))
                        .help(PROJECT_HELP)
                        .control(
                            Icon::new(IconName::ChevronRight)
                                .size(IconSize::Small)
                                .color(Color::Placeholder),
                        )
                        .on_press(None, move |window, cx| {
                            let page = page.clone();
                            view.update(cx, |view, cx| view.open_page(page, window, cx))
                        })
                });
            let card = SettingsCard::new(SeparatorInset::Tile).children(rows);
            parts.push(
                SettingsGroup::new("Projects")
                    .child(card)
                    .into_any_element(),
            );
        }
        scroll_body("settings-results", v_flex().children(parts))
    }
}
