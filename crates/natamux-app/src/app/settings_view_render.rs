//! How the Settings page draws: the nav with its search, the page header, the app's pages, the
//! search results, and the project page.

use std::rc::Rc;

use gpui::{ClickEvent, FontWeight, KeyDownEvent};
use ui::{Indicator, ListItem, ListItemSpacing, Switch, Tab, Tooltip, prelude::*};

use super::super::appearance_rules::{ThemeChoice, UiSize, parse_theme, parse_ui_size};
use super::super::settings_rules::{SearchResults, SettingRow, SettingsPage, page_title, search};
use super::super::shortcuts::AddProject;
use super::{
    APP_ITEMS, NAV_WIDTH, NavItem, PAGE_MAX, SettingsView, heading, nav_items, nav_matches, segment,
};
use crate::components::button_style::quiet_button;

/// A row of an app page: the label and its line at the left, the control at the right.
fn setting_row(row: SettingRow, control: AnyElement, cx: &App) -> AnyElement {
    h_flex()
        .w_full()
        .min_h(rems_from_px(28.0_f32))
        .py_3()
        .gap_6()
        .justify_between()
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .child(
            v_flex()
                .min_w_0()
                .gap_0p5()
                .child(Label::new(row.label()))
                .child(
                    Label::new(row.description())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// The body of an app page or of the results: it scrolls, its rows stay readable on a wide window.
fn scroll_body(id: &'static str, inner: impl IntoElement) -> AnyElement {
    v_flex()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px_6()
        .py_5()
        .child(div().w_full().max_w(rems_from_px(PAGE_MAX)).child(inner))
        .into_any_element()
}

fn section(text: &'static str) -> impl IntoElement {
    div().pt_5().pb_1().child(heading(text))
}

impl SettingsView {
    fn control(&mut self, row: SettingRow, cx: &mut Context<Self>) -> AnyElement {
        match row {
            SettingRow::Notify => self.notify_switch(cx),
            SettingRow::Theme => self.theme_control(cx),
            SettingRow::UiSize => self.size_control(cx),
            SettingRow::FontFamily => self.font.family_control(cx),
            SettingRow::FontSize => self.font.size_control(cx),
        }
    }

    fn row(&mut self, row: SettingRow, cx: &mut Context<Self>) -> AnyElement {
        let control = self.control(row, cx);
        setting_row(row, control, cx)
    }

    fn notify_switch(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = self.dashboard.read(cx).view().notify;
        let dashboard = self.dashboard.clone();
        let flip = Rc::new(move |cx: &mut App| {
            dashboard.update(cx, |dashboard, cx| dashboard.toggle_notify(cx))
        });
        let by_key = flip.clone();
        div()
            .id("settings-notify-stop")
            .track_focus(&self.notify_focus)
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                let keys = &event.keystroke;
                if matches!(keys.key.as_str(), "space" | "enter") && !keys.modifiers.modified() {
                    cx.stop_propagation();
                    by_key(cx);
                }
            })
            .child(Switch::new("settings-notify", on.into()).on_click(move |_, _, cx| flip(cx)))
            .into_any_element()
    }

    fn theme_control(&self, cx: &App) -> AnyElement {
        let theme = parse_theme(&self.dashboard.read(cx).view().theme);
        let selected = ThemeChoice::ALL
            .iter()
            .position(|choice| *choice == theme)
            .unwrap_or(0);
        let dashboard = self.dashboard.clone();
        segment(
            "settings-theme",
            &self.theme_focus,
            ThemeChoice::ALL.map(ThemeChoice::label),
            selected,
            move |index, _, cx| {
                let name = ThemeChoice::ALL[index].name();
                dashboard.update(cx, |dashboard, cx| dashboard.set_theme(name, cx))
            },
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
                let name = UiSize::ALL[index].name();
                dashboard.update(cx, |dashboard, cx| dashboard.set_ui_size(name, cx))
            },
        )
        .into_any_element()
    }

    fn general(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let notify = self.row(SettingRow::Notify, cx);
        scroll_body("settings-general", v_flex().child(notify))
    }

    fn appearance(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let rows = [SettingRow::Theme, SettingRow::UiSize].map(|row| self.row(row, cx));
        let font = [SettingRow::FontFamily, SettingRow::FontSize].map(|row| self.row(row, cx));
        let preview = self.font.preview(cx);
        scroll_body(
            "settings-appearance",
            v_flex()
                .child(section("Theme"))
                .children(rows)
                .child(section("Terminal font"))
                .children(font)
                .child(preview),
        )
    }

    /// The rows a search found, under their page, then the projects it found as links.
    fn results(
        &mut self,
        found: &SearchResults,
        query: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if found.is_empty() {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    Label::new(format!("No settings match \u{201c}{query}\u{201d}"))
                        .color(Color::Muted),
                )
                .into_any_element();
        }
        let mut parts: Vec<AnyElement> = Vec::new();
        for (page, title) in [
            (SettingsPage::General, "General"),
            (SettingsPage::Appearance, "Appearance"),
        ] {
            let rows: Vec<SettingRow> = found
                .rows
                .iter()
                .copied()
                .filter(|row| row.page() == page)
                .collect();
            if rows.is_empty() {
                continue;
            }
            parts.push(section(title).into_any_element());
            for row in rows {
                parts.push(self.row(row, cx));
            }
        }
        if !found.projects.is_empty() {
            parts.push(section("Projects").into_any_element());
        }
        for (index, (id, name)) in found.projects.iter().enumerate() {
            let page = SettingsPage::Project(id.clone());
            let link = quiet_button(("settings-result-project", index), name.clone())
                .tooltip(Tooltip::text(format!("Settings of {name}")))
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    view.open_page(page.clone(), window, cx)
                }));
            parts.push(div().py_1().child(link).into_any_element());
        }
        scroll_body("settings-results", v_flex().children(parts))
    }

    fn header(&self, title: String, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .flex_none()
            .h(Tab::container_height(cx))
            .pl_6()
            .pr_2()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(Label::new(title).weight(FontWeight::SEMIBOLD).truncate()),
            )
            .child(
                IconButton::new("settings-close", IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Close settings"))
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.dashboard
                            .update(cx, |dashboard, cx| dashboard.close_settings(cx))
                    })),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn nav_row(
        &self,
        index: usize,
        item: &NavItem,
        label: &str,
        page: &SettingsPage,
        found: Option<&SearchResults>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = found.is_none() && matches!(item, NavItem::Page(shown) if shown == page);
        let dim = found.is_some_and(|found| !nav_matches(item, found));
        let unsaved = match item {
            NavItem::Page(SettingsPage::Project(id)) => self
                .project_page(id)
                .is_some_and(|page| page.read(cx).dirty(cx)),
            _ => false,
        };
        let stop = self.nav_stops[index].clone();
        let pick = item.clone();
        let mark = unsaved.then(|| {
            div()
                .id(("settings-unsaved", index))
                .tooltip(Tooltip::text("Unsaved changes"))
                .child(Indicator::dot().color(Color::Accent))
        });
        div()
            .track_focus(&stop)
            .child(
                ListItem::new(("settings-nav", index))
                    .inset(true)
                    .spacing(ListItemSpacing::Sparse)
                    .toggle_state(selected)
                    .focused(stop.is_focused(window))
                    .end_slot::<AnyElement>(mark.map(IntoElement::into_any_element))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.pick(&pick, window, cx)
                    }))
                    .child(
                        Label::new(label.to_owned())
                            .color(if dim { Color::Muted } else { Color::Default })
                            .truncate(),
                    ),
            )
            .into_any_element()
    }

    /// As high as the page header and the sidebar filter, so one line runs under both.
    fn search_bar(&self, cx: &App) -> impl IntoElement + use<> {
        h_flex()
            .flex_none()
            .h(Tab::container_height(cx))
            .px_3()
            .gap(rems_from_px(6.0_f32))
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                Icon::new(IconName::MagnifyingGlass)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(div().min_w_0().flex_1().child(self.search.clone()))
    }

    fn nav(
        &mut self,
        page: &SettingsPage,
        found: Option<&SearchResults>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let items = nav_items(&self.dashboard.read(cx).data.repos);
        while self.nav_stops.len() < items.len() {
            self.nav_stops.push(cx.focus_handle());
        }
        let mut rows: Vec<AnyElement> = items
            .iter()
            .enumerate()
            .map(|(index, (item, label))| self.nav_row(index, item, label, page, found, window, cx))
            .collect();
        let projects = rows.split_off(APP_ITEMS.min(rows.len()));
        let colors = cx.theme().colors();
        let (panel, border) = (colors.panel_background, colors.border);
        let add = IconButton::new("settings-add-project", IconName::Plus)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Add a project"))
            .on_click(|_, window, cx| window.dispatch_action(Box::new(AddProject), cx));
        let sub_header = |text: &'static str| {
            h_flex()
                .h(rems_from_px(24.0_f32))
                .px_1p5()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
        };
        v_flex()
            .w(rems_from_px(NAV_WIDTH))
            .flex_none()
            .h_full()
            .bg(panel)
            .border_r_1()
            .border_color(border)
            .on_key_down(cx.listener(Self::nav_key))
            .child(self.search_bar(cx))
            .child(
                v_flex()
                    .id("settings-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_1p5()
                    .child(sub_header("App"))
                    .children(rows)
                    .child(
                        h_flex()
                            .pt_3()
                            .justify_between()
                            .child(sub_header("Projects"))
                            .child(add),
                    )
                    .when(projects.is_empty(), |this| {
                        this.child(
                            div()
                                .px_1p5()
                                .py_1()
                                .child(Label::new("No projects yet").color(Color::Muted)),
                        )
                    })
                    .children(projects),
            )
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.page(cx);
        if let SettingsPage::Project(id) = &page {
            self.ensure_project(id, window, cx);
        }
        let query = self.query(cx);
        let repos = self.dashboard.read(cx).data.repos.clone();
        let found = search(&query, &repos);
        let title = match &found {
            Some(_) => "Results".to_owned(),
            None => page_title(&page, &repos),
        };
        let body = match (&found, &page) {
            (Some(found), _) => self.results(found, query.trim(), cx),
            (None, SettingsPage::General) => self.general(cx),
            (None, SettingsPage::Appearance) => self.appearance(cx),
            (None, SettingsPage::Project(id)) => self
                .project_page(id)
                .map_or_else(|| div().into_any_element(), IntoElement::into_any_element),
        };
        let nav = self.nav(&page, found.as_ref(), window, cx);
        let header = self.header(title, cx);
        h_flex()
            .size_full()
            .track_focus(&self.focus)
            .bg(cx.theme().colors().editor_background)
            .child(nav)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(header)
                    .child(body),
            )
    }
}
