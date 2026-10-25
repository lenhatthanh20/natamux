//! The app's own settings (theme, UI size, terminal font), the Settings page that shows them, and
//! the daemon the dashboard talks to. The settings are kept in `view.json` and applied at once
//! (`app/appearance.rs`).

use std::sync::Arc;

use futures::StreamExt;
use serde_json::Value;
use ui::prelude::*;

use super::super::appearance;
use super::super::dashboard_data::{DashboardData, Link};
use super::super::settings_rules::{SettingsFocus, SettingsPage, page_to_open};
use super::super::source::{Feed, Source};
use super::{Dashboard, DashboardChange};

impl Dashboard {
    pub fn set_theme(&mut self, theme: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.theme = theme.to_owned());
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
        cx.notify();
    }

    /// Main shows again what it showed under Settings.
    pub fn close_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings.take().is_some() {
            cx.emit(DashboardChange::SettingsClosed);
            cx.notify();
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
        self._feed = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |this, cx| this.apply(event, cx)).is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    /// After Sign in: the same views follow the daemon with the new token.
    pub fn reconnect(&mut self, source: Arc<Source>, cx: &mut Context<Self>) {
        self.source = Some(source);
        self.data = DashboardData::default();
        self.watch(cx);
    }
}
