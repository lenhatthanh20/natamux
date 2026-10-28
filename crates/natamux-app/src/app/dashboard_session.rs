//! The app's own settings (theme, UI size, terminal font), the Settings page that shows them, and
//! the daemon the dashboard talks to. The settings are kept in `view.json` and applied at once
//! (`app/appearance.rs`).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use gpui::{Entity, Subscription};
use serde_json::Value;
use ui::prelude::*;

use super::super::appearance;
use super::super::dashboard_data::{DashboardData, Link, Part, Seen};
use super::super::flow_bench;
use super::super::settings_rules::{SettingsFocus, SettingsPage, page_to_open};
use super::super::source::{Feed, Rows, Source};
use super::super::theme_catalog::ThemeKind;
use super::super::view_state::ViewState;
use super::{Dashboard, DashboardChange};

/// Events that came together apply in one update, at most this many.
const BATCH_MAX: usize = 64;

/// The view state `start` read once (main.rs), and the folder it is saved to; None: never
/// saved, as for the fake data, which reuses real ids.
#[derive(Default)]
pub struct SavedView {
    pub view: ViewState,
    pub folder: Option<PathBuf>,
}

impl Dashboard {
    pub fn set_theme(&mut self, theme: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.theme = theme.to_owned());
        appearance::apply(&self.view, cx);
    }

    /// The theme the light or the dark look shows.
    pub fn set_theme_of(&mut self, kind: ThemeKind, name: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.themes = view.themes.with(kind, name));
        appearance::apply(&self.view, cx);
    }

    pub fn set_ui_size(&mut self, size: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.ui_size = size.to_owned());
        appearance::apply(&self.view, cx);
    }

    /// None: the terminals use the built-in font (Zed's buffer font).
    pub fn set_terminal_font(&mut self, font: Option<Value>, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.terminal_font = font);
        appearance::apply(&self.view, cx);
    }

    /// Shows Settings over main on `page`; None: the page on screen, else the last one. Root
    /// moves the focus (`focus`).
    pub fn open_settings(
        &mut self,
        page: Option<SettingsPage>,
        focus: SettingsFocus,
        cx: &mut Context<Self>,
    ) {
        let page = page_to_open(page, self.settings.as_ref(), &self.last_settings_page);
        self.set_settings_page(page, cx);
        cx.emit(DashboardChange::SettingsOpened(focus));
    }

    /// The Settings button of a project: its page, with the focus on it.
    pub fn open_project_settings(&mut self, repo_id: &str, cx: &mut Context<Self>) {
        let page = SettingsPage::Project(repo_id.to_owned());
        self.open_settings(Some(page), SettingsFocus::Page, cx);
    }

    /// A page of the Settings nav.
    pub fn set_settings_page(&mut self, page: SettingsPage, cx: &mut Context<Self>) {
        self.last_settings_page = page.clone();
        self.settings = Some(page);
        self.touch_view(cx);
    }

    /// Main shows again what it showed under Settings.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings.take().is_some() {
            cx.emit(DashboardChange::SettingsClosed);
            self.touch_view(cx);
        }
    }

    /// Follows the feed again from the start: Retry of "Could not load tasks".
    pub fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let Feed { mut events, rows } = source.watch_dashboard();
        self.rows = Some(rows);
        self.data.link = Link::Loading;
        self.data.gens.bump(Part::Link);
        self._feed = Some(cx.spawn(async move |this, cx| {
            while let Some(first) = events.next().await {
                // The views follow the notify of the whole batch once.
                let mut batch = vec![first];
                while batch.len() < BATCH_MAX {
                    match events.try_recv() {
                        Ok(event) => batch.push(event),
                        Err(_) => break,
                    }
                }
                // `--bench-flows`: one sample per batch; its wall time covers the observers it
                // wakes.
                let timed = crate::perf::stats::timing_events().then(Instant::now);
                let applied = this.update(cx, |this, cx| {
                    for event in batch {
                        this.apply(event, cx);
                    }
                });
                if applied.is_err() {
                    break;
                }
                if let Some(started) = timed {
                    cx.update(|cx| flow_bench::event_applied(started.elapsed(), cx));
                }
            }
        }));
        self.touch_view(cx);
    }

    /// After Sign in, or when the app's own daemon answered: the same views follow it.
    pub fn reconnect(&mut self, source: Arc<Source>, cx: &mut Context<Self>) {
        self.source = Some(source);
        // New data: every part changed, also for views that saw the old one.
        let mut gens = self.data.gens;
        gens.bump_all();
        self.data = DashboardData::default();
        self.data.gens = gens;
        self.watch(cx);
    }

    /// The app's own daemon did not start: the window says why, as for a start problem.
    pub fn start_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        self.data.link = Link::Down {
            reason,
            retrying: false,
        };
        self.data.gens.bump(Part::Link);
        cx.notify();
    }

    /// The notify of the dashboard's own state (filter, sort, selection, open task, view state,
    /// settings, toasts): views that read it draw again.
    pub(super) fn touch_view(&mut self, cx: &mut Context<Self>) {
        self.data.gens.bump(Part::View);
        cx.notify();
    }
}

impl Dashboard {
    /// Draws the view `V` again when one of `parts` changed, not on every dashboard notify.
    pub fn redraw_on<V: 'static>(
        dashboard: &Entity<Self>,
        parts: &'static [Part],
        cx: &mut Context<V>,
    ) -> Subscription {
        let mut seen = Seen::default();
        cx.observe(dashboard, move |_, dashboard, cx| {
            let gens = dashboard.read(cx).data.gens;
            if seen.changed(&gens, parts) {
                cx.notify();
            }
        })
    }

    /// One event of the feed, as `watch` applies it, for the tests of the views.
    #[cfg(test)]
    pub fn take_event(&mut self, event: natamux_client::DashboardEvent, cx: &mut Context<Self>) {
        self.apply(event, cx);
    }

    /// The feed's fallback poll waits while no window is active (window_watch.rs).
    pub fn set_feed_active(&self, active: bool) {
        if let Some(Rows::Daemon(rows)) = &self.rows {
            rows.set_active(active);
        }
    }
}
