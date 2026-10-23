//! The app's own settings (theme, UI size, terminal font) and the daemon the dashboard talks to.
//! The settings are kept in `view.json` and applied at once (`app/appearance.rs`).

use std::sync::Arc;

use serde_json::Value;
use ui::prelude::*;

use super::super::appearance;
use super::super::dashboard_data::DashboardData;
use super::super::source::Source;
use super::Dashboard;

impl Dashboard {
    pub fn set_theme(&mut self, theme: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.theme = theme.to_owned());
        appearance::apply(&self.view, cx);
    }

    pub fn set_ui_size(&mut self, size: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.ui_size = size.to_owned());
        appearance::apply(&self.view, cx);
    }

    /// None: the terminals use Ghostty's font, else the built-in one.
    pub fn set_terminal_font(&mut self, font: Option<Value>, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.terminal_font = font);
        appearance::apply(&self.view, cx);
    }

    /// After Sign in: the same views follow the daemon with the new token.
    pub fn reconnect(&mut self, source: Arc<Source>, cx: &mut Context<Self>) {
        self.source = Some(source);
        self.data = DashboardData::default();
        self.watch(cx);
    }
}
