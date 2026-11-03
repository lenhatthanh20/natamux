//! The Settings page (T-0174; docs/rust-phase3.md "Lean layout"): a nav with a search, the app's
//! pages (General, Appearance, Agents) and a page per project (`ProjectSettingsPage`). It fills
//! the window body over main. App settings apply at once; a project page keeps its draft until Save
//! or Discard, also while other pages show. The page owns its keys as a dialog does: Esc,
//! Mod+Enter and a Tab ring.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use editor::{Editor, EditorEvent};
use gpui::{
    Entity, FocusHandle, Focusable, FontWeight, KeyDownEvent, KeystrokeEvent, Subscription,
};
use natamux_proto::projects::repo::Repo;
use ui::{ToggleButtonGroup, ToggleButtonGroupStyle, ToggleButtonSimple, prelude::*};

use super::dashboard::Dashboard;
use super::dashboard_data::{Part, Seen};
use super::settings_agents::AgentRows;
use super::settings_font::FontRows;
use super::settings_rules::{
    SearchResults, SettingRow, SettingsFocus, SettingsPage, page_title, search, shown_page,
};
use super::shortcut_sheet;
use super::update::UpdateChanged;
use crate::components::dialog::DialogLayer;
use crate::components::form::text_editor;
use crate::components::input_rules::{DialogKey, dialog_key, ring_step, segment_step};
use crate::components::menu::press_of;
use crate::projects::project_settings_page::{Discarded, ProjectSettingsPage};

const NAV_WIDTH: f32 = 220.;
/// The rows of the app's pages stay this wide on a wide window.
const PAGE_MAX: f32 = 760.;
/// General, Appearance, Agents and Keyboard shortcuts come before the projects in the nav.
const APP_ITEMS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
enum NavItem {
    Page(SettingsPage),
    /// Opens the shortcut sheet; not a page.
    Shortcuts,
}

fn nav_items(repos: &[Repo]) -> Vec<(NavItem, String)> {
    let mut items = vec![
        (NavItem::Page(SettingsPage::General), "General".to_owned()),
        (
            NavItem::Page(SettingsPage::Appearance),
            "Appearance".to_owned(),
        ),
        (NavItem::Page(SettingsPage::Agents), "Agents".to_owned()),
        (NavItem::Shortcuts, "Keyboard shortcuts".to_owned()),
    ];
    items.extend(repos.iter().map(|repo| {
        let page = SettingsPage::Project(repo.id.clone());
        (NavItem::Page(page), repo.name.clone())
    }));
    items
}

/// A nav row stays bright while a search finds something on its page.
fn nav_matches(item: &NavItem, found: &SearchResults) -> bool {
    match item {
        NavItem::Page(SettingsPage::Project(id)) => {
            found.projects.iter().any(|(project, _)| project == id)
        }
        NavItem::Page(page) => found.rows.iter().any(|row| &row.page() == page),
        NavItem::Shortcuts => false,
    }
}

pub struct SettingsView {
    dashboard: Entity<Dashboard>,
    focus: FocusHandle,
    search: Entity<Editor>,
    theme_focus: FocusHandle,
    light_focus: FocusHandle,
    dark_focus: FocusHandle,
    size_focus: FocusHandle,
    notify_focus: FocusHandle,
    sidebar_ids_focus: FocusHandle,
    update_focus: FocusHandle,
    check_focus: FocusHandle,
    pub(super) font: FontRows,
    pub(super) agents: AgentRows,
    /// One stop per nav row. Grown while drawing.
    nav_stops: Vec<FocusHandle>,
    /// The project pages made so far, with their Discard subscription. They keep their drafts.
    projects: BTreeMap<String, (Entity<ProjectSettingsPage>, Subscription)>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = text_editor("Search settings", None, window, cx);
        let typing = cx.subscribe(&search, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                cx.notify();
            }
        });
        // A project that is gone takes its page and draft with it. The pages show the projects
        // and the app settings of the view state.
        let mut seen = Seen::default();
        let follow = cx.observe(&dashboard, move |view, dashboard, cx| {
            let gens = dashboard.read(cx).data.gens;
            let changed = seen.take(&gens);
            if !changed.any(&[Part::Repos, Part::View]) {
                return;
            }
            let ids: BTreeSet<String> = dashboard
                .read(cx)
                .data
                .repos
                .iter()
                .map(|repo| repo.id.clone())
                .collect();
            view.projects.retain(|id, _| ids.contains(id));
            cx.notify();
        });
        let font = FontRows::new(dashboard.clone(), window, cx);
        let agents = AgentRows::new(dashboard.clone(), cx);
        let keys = Self::watch_keys(cx);
        let updates = cx.observe_global::<UpdateChanged>(|_, cx| cx.notify());
        Self {
            dashboard,
            focus: cx.focus_handle(),
            search,
            theme_focus: cx.focus_handle().tab_stop(true),
            light_focus: cx.focus_handle().tab_stop(true),
            dark_focus: cx.focus_handle().tab_stop(true),
            size_focus: cx.focus_handle().tab_stop(true),
            notify_focus: cx.focus_handle(),
            sidebar_ids_focus: cx.focus_handle(),
            update_focus: cx.focus_handle(),
            check_focus: cx.focus_handle(),
            font,
            agents,
            nav_stops: Vec::new(),
            projects: BTreeMap::new(),
            _subscriptions: vec![typing, follow, keys, updates],
        }
    }

    /// Esc, Mod+Enter and Tab while focus is in Settings and no dialog is over it, before the
    /// bindings of the views inside, as the dialog layer does for a dialog.
    fn watch_keys(cx: &mut Context<Self>) -> Subscription {
        let view = cx.weak_entity();
        cx.intercept_keystrokes(move |event: &KeystrokeEvent, window, cx| {
            let Some(view) = view.upgrade() else {
                return;
            };
            let this = view.read(cx);
            let shown = this.dashboard.read(cx).settings.is_some();
            if !shown || DialogLayer::is_open(cx) || !this.focus.contains_focused(window, cx) {
                return;
            }
            let Some(key) = dialog_key(press_of(&event.keystroke)) else {
                return;
            };
            cx.stop_propagation();
            window.defer(cx, move |window, cx| {
                view.update(cx, |view, cx| view.on_key(key, window, cx));
            });
        })
    }

    /// Moves the focus after Settings opened (Root, on `SettingsOpened`).
    pub fn show(&mut self, focus: SettingsFocus, window: &mut Window, cx: &mut Context<Self>) {
        let target = match (self.page(cx), focus) {
            (_, SettingsFocus::Font) => self.font.family.focus_handle(cx),
            (SettingsPage::Project(id), SettingsFocus::Page) => {
                self.ensure_project(&id, window, cx).focus_handle(cx)
            }
            _ => self.search.focus_handle(cx),
        };
        window.focus(&target, cx);
        cx.notify();
    }

    /// The page of a project, once Settings showed it.
    pub fn project_page(&self, id: &str) -> Option<Entity<ProjectSettingsPage>> {
        self.projects.get(id).map(|(page, _)| page.clone())
    }

    /// Types `query` into the search (the shots).
    pub fn set_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |editor, cx| editor.set_text(query, window, cx));
    }

    fn page(&self, cx: &App) -> SettingsPage {
        let dashboard = self.dashboard.read(cx);
        dashboard
            .settings
            .as_ref()
            .map_or(SettingsPage::Appearance, |page| {
                shown_page(page, &dashboard.data.repos)
            })
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).text(cx)
    }

    fn ensure_project(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ProjectSettingsPage> {
        if let Some(page) = self.project_page(id) {
            return page;
        }
        let page = SettingsPage::Project(id.to_owned());
        let name = page_title(&page, &self.dashboard.read(cx).data.repos);
        let (dashboard, repo_id) = (self.dashboard.clone(), id.to_owned());
        let page = cx.new(|cx| ProjectSettingsPage::new(dashboard, repo_id, name, window, cx));
        let repo = id.to_owned();
        let discarded = cx.subscribe_in(&page, window, move |_, _, _: &Discarded, window, cx| {
            // Dropping the page drops this subscription: done after the event.
            let (view, repo) = (cx.entity(), repo.clone());
            window.defer(cx, move |window, cx| {
                view.update(cx, |view, cx| view.reopen_project(&repo, window, cx));
            });
        });
        self.projects
            .insert(id.to_owned(), (page.clone(), discarded));
        page
    }

    /// After Discard: a new page from the file.
    fn reopen_project(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.projects.remove(id);
        let page = self.ensure_project(id, window, cx);
        window.focus(&page.focus_handle(cx), cx);
        cx.notify();
    }

    /// The project page on screen; None while a search shows its results.
    fn shown_project(&self, cx: &App) -> Option<Entity<ProjectSettingsPage>> {
        match self.page(cx) {
            SettingsPage::Project(id) if self.query(cx).trim().is_empty() => self.project_page(&id),
            _ => None,
        }
    }

    fn on_key(&mut self, key: DialogKey, window: &mut Window, cx: &mut Context<Self>) {
        let project = self.shown_project(cx);
        match key {
            DialogKey::Cancel => {
                if let Some(page) = &project
                    && (page.read(cx).busy()
                        || page.update(cx, |page, cx| page.closes_layer(window, cx)))
                {
                    return;
                }
                let searching = !self.query(cx).is_empty();
                if searching && self.search.focus_handle(cx).contains_focused(window, cx) {
                    self.search
                        .update(cx, |editor, cx| editor.clear(window, cx));
                    return;
                }
                self.dashboard
                    .update(cx, |dashboard, cx| dashboard.close_settings(cx));
            }
            DialogKey::Submit => {
                if let Some(page) = project.filter(|page| !page.read(cx).busy()) {
                    page.update(cx, |page, cx| page.save(window, cx));
                }
            }
            DialogKey::Next | DialogKey::Previous => {
                let ring = self.tab_ring(cx);
                let current = ring.iter().position(|h| h.contains_focused(window, cx));
                let back = key == DialogKey::Previous;
                if let Some(next) = ring_step(ring.len(), current, back) {
                    window.focus(&ring[next], cx);
                }
            }
        }
    }

    /// The search, the nav rows, then the stops of the page or of the results.
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let repos = &self.dashboard.read(cx).data.repos;
        let mut ring = vec![self.search.focus_handle(cx)];
        ring.extend(self.nav_stops.iter().take(nav_items(repos).len()).cloned());
        if let Some(found) = search(&self.query(cx), repos) {
            for row in found.rows {
                ring.extend(self.row_stops(row, cx));
            }
            return ring;
        }
        match self.page(cx) {
            SettingsPage::General => ring.extend([
                self.notify_focus.clone(),
                self.update_focus.clone(),
                self.check_focus.clone(),
            ]),
            SettingsPage::Appearance => {
                ring.extend([
                    self.theme_focus.clone(),
                    self.light_focus.clone(),
                    self.dark_focus.clone(),
                    self.size_focus.clone(),
                    self.sidebar_ids_focus.clone(),
                ]);
                ring.extend(self.font.stops(cx));
            }
            SettingsPage::Agents => ring.extend(self.agents.stops(cx)),
            SettingsPage::Project(id) => {
                if let Some(page) = self.project_page(&id) {
                    ring.extend(page.read(cx).tab_ring(cx));
                }
            }
        }
        ring
    }

    fn row_stops(&self, row: SettingRow, cx: &App) -> Vec<FocusHandle> {
        match row {
            SettingRow::Notify => vec![self.notify_focus.clone()],
            SettingRow::UpdatesVersion => Vec::new(),
            SettingRow::UpdatesAuto => vec![self.update_focus.clone()],
            SettingRow::UpdatesCheck => vec![self.check_focus.clone()],
            SettingRow::Theme => vec![self.theme_focus.clone()],
            SettingRow::LightTheme => vec![self.light_focus.clone()],
            SettingRow::DarkTheme => vec![self.dark_focus.clone()],
            SettingRow::UiSize => vec![self.size_focus.clone()],
            SettingRow::SidebarIds => vec![self.sidebar_ids_focus.clone()],
            SettingRow::FontFamily => vec![self.font.family.focus_handle(cx)],
            SettingRow::FontSize => self.font.size_stops(cx),
            SettingRow::CustomAgents => Vec::new(),
        }
    }

    /// A nav row or a search result: shows the page, and drops the search.
    fn open_page(&mut self, page: SettingsPage, window: &mut Window, cx: &mut Context<Self>) {
        self.search
            .update(cx, |editor, cx| editor.clear(window, cx));
        if let SettingsPage::Project(id) = &page {
            self.ensure_project(id, window, cx);
        }
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_settings_page(page, cx));
    }

    fn pick(&mut self, item: &NavItem, window: &mut Window, cx: &mut Context<Self>) {
        match item {
            NavItem::Page(page) => self.open_page(page.clone(), window, cx),
            NavItem::Shortcuts => shortcut_sheet::open(window, cx),
        }
    }

    /// Up and Down move over the nav rows; Enter and Space open the focused one.
    fn nav_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(at) = self
            .nav_stops
            .iter()
            .position(|stop| stop.is_focused(window))
        else {
            return;
        };
        let items = nav_items(&self.dashboard.read(cx).data.repos);
        let next = match event.keystroke.key.as_str() {
            "up" => at.checked_sub(1),
            "down" => Some(at + 1).filter(|next| *next < items.len()),
            "enter" | "space" => {
                if let Some((item, _)) = items.get(at) {
                    cx.stop_propagation();
                    self.pick(item, window, cx);
                }
                return;
            }
            _ => return,
        };
        cx.stop_propagation();
        if let Some(next) = next.and_then(|next| self.nav_stops.get(next)) {
            window.focus(next, cx);
        }
    }
}

#[path = "settings_view_render.rs"]
mod render;

/// Text-only segmented control. Arrow keys and Home / End pick, as in `Segmented`.
fn segment<const N: usize>(
    name: &'static str,
    focus: &FocusHandle,
    labels: [&'static str; N],
    selected: usize,
    on_change: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let on_change = Rc::new(on_change);
    let buttons: [ToggleButtonSimple; N] = std::array::from_fn(|index| {
        let pick = on_change.clone();
        ToggleButtonSimple::new(labels[index], move |_, window, cx| pick(index, window, cx))
            .selected(index == selected)
    });
    div()
        .id(name)
        .flex_none()
        .track_focus(focus)
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if let Some(next) = segment_step(N, selected, &event.keystroke.key) {
                if next != selected {
                    on_change(next, window, cx);
                }
                cx.stop_propagation();
            }
        })
        .child(
            ToggleButtonGroup::single_row(name, buttons)
                .style(ToggleButtonGroupStyle::Outlined)
                .selected_index(selected)
                .auto_width(),
        )
}

fn heading(text: &'static str) -> Label {
    Label::new(text)
        .size(LabelSize::XSmall)
        .weight(FontWeight::MEDIUM)
        .color(Color::Muted)
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
