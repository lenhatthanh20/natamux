//! How the Settings popup draws (layout-b 1, 2): the nav with its search, tiles and avatars, the
//! 72 px page header, the page or the search results, and the Agents page's bottom bar. A project
//! page draws its own header and bar.

use gpui::{ClickEvent, FontWeight};
use ui::{Tooltip, prelude::*};

use super::super::settings_rules::{
    SearchResults, SettingsPage, page_summary, page_tile, page_title, results_summary, search,
};
use super::super::shortcuts::{CloseSettings, SETTINGS_CONTEXT};
use super::nav::{APP_ITEMS, NavItem, NavTile, nav_items, nav_state, nav_tile};
use super::pages::scroll_body;
use super::{NAV_WIDTH, SettingsView};
use crate::components::button_style::button_icon;
use crate::components::colors::{TileColor, accent};
use crate::components::dialog::POPUP_RADIUS;
use crate::components::settings_tile::{PageHeader, SettingsTile, TileSize};
use crate::components::text_size::row_text;

fn nav_tile_view(tile: NavTile, dim: bool) -> SettingsTile {
    let tile = match tile {
        NavTile::Icon(icon, color) => SettingsTile::icon(button_icon(icon), color, TileSize::Nav),
        NavTile::Avatar(name) => SettingsTile::avatar(&name, TileSize::Nav),
        NavTile::Dashed => SettingsTile::dashed(button_icon("plus"), TileSize::Nav),
    };
    tile.dim(dim)
}

/// The nav's "App" and "Projects" heads: 24 px, 12 px medium placeholder text.
fn nav_head(text: &'static str) -> Div {
    h_flex()
        .h(rems_from_px(24.0_f32))
        .px(rems_from_px(8.0_f32))
        .child(
            Label::new(text)
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Placeholder),
        )
}

impl SettingsView {
    fn header(
        &self,
        page: &SettingsPage,
        found: Option<&SearchResults>,
        query: &str,
        cx: &App,
    ) -> AnyElement {
        let dashboard = self.dashboard.clone();
        let close = move |_: &mut Window, cx: &mut App| {
            dashboard.update(cx, |dashboard, cx| dashboard.close_settings(cx))
        };
        let tile = |icon: &str, color: TileColor| {
            SettingsTile::icon(button_icon(icon), color, TileSize::Header)
        };
        let header = match found {
            Some(found) => PageHeader::new(tile("search", TileColor::Gray), "Results", close)
                .summary(results_summary(found, query)),
            None => {
                let (icon, color) = page_tile(page).unwrap_or(("settings", TileColor::Gray));
                let title = page_title(page, &self.dashboard.read(cx).data.repos);
                PageHeader::new(tile(icon, color), title, close).summary(page_summary(page))
            }
        };
        header.into_any_element()
    }

    fn unsaved(&self, item: &NavItem, cx: &App) -> bool {
        match item {
            NavItem::Page(SettingsPage::Project(id)) => self
                .project_page(id)
                .is_some_and(|page| page.read(cx).dirty(cx)),
            NavItem::Page(SettingsPage::Agents) => self.agents.unsaved(),
            _ => false,
        }
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
        let (selected, dim) = nav_state(item, page, found);
        let stop = self.nav_stops[index].clone();
        let colors = cx.theme().colors();
        let (hover, on, ring) = (
            colors.ghost_element_hover,
            colors.ghost_element_selected,
            colors.border_focused,
        );
        let quiet = dim || *item == NavItem::AddProject;
        let dot = self.unsaved(item, cx).then(|| {
            div()
                .id(("settings-unsaved", index))
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .bg(accent(cx))
                .tooltip(Tooltip::text("Unsaved changes"))
        });
        let shortcuts = *item == NavItem::Shortcuts;
        let pick = item.clone();
        h_flex()
            .id(("settings-nav", index))
            .h(rems_from_px(32.0_f32))
            .gap(rems_from_px(10.0_f32))
            .px(rems_from_px(8.0_f32))
            .rounded(rems_from_px(6.0_f32))
            .border_1()
            .border_color(if stop.is_focused(window) {
                ring
            } else {
                gpui::transparent_black()
            })
            .track_focus(&stop)
            .when(selected, |row| row.bg(on))
            .hover(move |style| style.bg(hover))
            .when(shortcuts, |row| {
                row.tooltip(Tooltip::text("Opens the shortcut sheet"))
            })
            .on_click(
                cx.listener(move |view, _: &ClickEvent, window, cx| view.pick(&pick, window, cx)),
            )
            .child(nav_tile_view(nav_tile(item, label), dim))
            .child(
                div().flex_1().min_w_0().child(
                    Label::new(label.to_owned())
                        .size(row_text())
                        .color(if quiet {
                            Color::Placeholder
                        } else {
                            Color::Default
                        })
                        .truncate(),
                ),
            )
            .children(dot)
            .when(shortcuts, |row| {
                row.child(
                    Icon::new(IconName::ArrowUpRight)
                        .size(IconSize::XSmall)
                        .color(Color::Placeholder),
                )
            })
            .into_any_element()
    }

    fn search_bar(&self, query: &str, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let clear = (!query.is_empty()).then(|| {
            IconButton::new("settings-search-clear", IconName::Close)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text("Clear"))
                .on_click(
                    cx.listener(|view, _: &ClickEvent, window, cx| view.clear_search(window, cx)),
                )
        });
        h_flex()
            .flex_none()
            .h(rems_from_px(34.0_f32))
            .px(rems_from_px(12.0_f32))
            .gap(rems_from_px(8.0_f32))
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Icon::new(IconName::MagnifyingGlass)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(div().min_w_0().flex_1().child(self.search.clone()))
            .children(clear)
    }

    fn nav(
        &mut self,
        page: &SettingsPage,
        found: Option<&SearchResults>,
        query: &str,
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
        // The last row is always "Add project…"; the projects sit between.
        let add_project = rows.pop();
        let mut projects = rows.split_off(APP_ITEMS.min(rows.len()));
        let gap_after_head = |row: AnyElement| div().mt(rems_from_px(2.0_f32)).child(row);
        let app: Vec<AnyElement> = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| match index {
                0 => gap_after_head(row).into_any_element(),
                _ => row,
            })
            .collect();
        let no_projects = projects.is_empty();
        if !no_projects {
            let first = projects.remove(0);
            projects.insert(0, gap_after_head(first).into_any_element());
        }
        let colors = cx.theme().colors();
        let (panel, border) = (colors.panel_background, colors.border);
        let add = IconButton::new("settings-add-project", IconName::Plus)
            .size(ButtonSize::Compact)
            .icon_size(IconSize::XSmall)
            .tooltip(Tooltip::text("Add a project"))
            .on_click(|_, window, cx| {
                window.dispatch_action(Box::new(super::super::shortcuts::AddProject), cx)
            });
        v_flex()
            .w(rems_from_px(NAV_WIDTH))
            .flex_none()
            .h_full()
            .rounded_l(px(POPUP_RADIUS - 1.))
            .bg(panel)
            .border_r_1()
            .border_color(border)
            .on_key_down(cx.listener(Self::nav_key))
            .child(self.search_bar(query, cx))
            .child(
                v_flex()
                    .id("settings-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(rems_from_px(8.0_f32))
                    .child(nav_head("App"))
                    .children(app)
                    .child(div().h(rems_from_px(12.0_f32)))
                    .child(nav_head("Projects").child(div().flex_1()).child(add))
                    .when(no_projects, |this| {
                        this.child(
                            div()
                                .px(rems_from_px(8.0_f32))
                                .py(rems_from_px(4.0_f32))
                                .child(Label::new("No projects yet").color(Color::Placeholder)),
                        )
                    })
                    .children(projects)
                    .children(add_project),
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
        let query = query.trim();
        let repos = self.dashboard.read(cx).data.repos.clone();
        let found = search(query, &repos);
        let project = match (&found, &page) {
            (None, SettingsPage::Project(id)) => self.project_page(id),
            _ => None,
        };
        // A project page draws its own header and bottom bar.
        let main: Vec<AnyElement> = match (&found, project) {
            (_, Some(project)) => vec![project.into_any_element()],
            (Some(found), None) => vec![
                self.header(&page, Some(found), query, cx),
                self.results(found, query, cx),
            ],
            (None, None) => {
                let header = self.header(&page, None, query, cx);
                match &page {
                    SettingsPage::Agents => {
                        self.agents.sync(window, cx);
                        let content = self.agents.content(cx);
                        vec![
                            header,
                            scroll_body("settings-agents", content),
                            self.agents.bottom_bar(cx),
                        ]
                    }
                    SettingsPage::Project(_) => vec![header, div().flex_1().into_any_element()],
                    app => {
                        let body = self.app_page(app, cx);
                        vec![header, body]
                    }
                }
            }
        };
        let nav = self.nav(&page, found.as_ref(), query, window, cx);
        h_flex()
            .size_full()
            .key_context(SETTINGS_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|view, _: &CloseSettings, _, cx| {
                view.dashboard
                    .update(cx, |dashboard, cx| dashboard.close_settings(cx))
            }))
            .rounded(px(POPUP_RADIUS - 1.))
            .bg(cx.theme().colors().editor_background)
            .child(nav)
            .child(v_flex().flex_1().min_w_0().h_full().children(main))
    }
}
