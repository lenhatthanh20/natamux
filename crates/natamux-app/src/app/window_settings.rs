//! Screen 7's commands on Root: the palettes, the shortcut sheet, the settings, the theme and the
//! UI size, the project dialogs, and Sign in when the daemon has no token for the app.

use std::collections::BTreeSet;

use gpui::Focusable;
use ui::prelude::*;

use super::Root;
use crate::app::appearance_rules::{ThemeChoice, UiSize};
use crate::app::shortcuts::{
    AddProject, CheckForUpdates, ImportIssues, OpenJump, OpenPalette, OpenSettings,
    RestartToUpdate, ShowShortcuts, ToggleAutomaticUpdates, UseCompactSize, UseDarkTheme,
    UseDefaultSize, UseLargeSize, UseLightTheme, UseSystemTheme,
};
use crate::app::sign_in::{SignIn, SignedIn, access_file};
use crate::app::{settings_dialog, shortcut_sheet};
use crate::palette::palette_layer::PaletteLayer;
use crate::palette::palette_rows::{PaletteMode, PaletteScope};
use crate::projects::{issue_import, projects_dialog};
use crate::tasks::task_view_rules::task_repo;
use crate::terminals::terminal_cache::{Owner, TerminalCache};

impl Root {
    /// The actions of this file, on the root element.
    pub(super) fn settings_actions(shell: Div, cx: &mut Context<Self>) -> Div {
        shell
            .on_action(cx.listener(|root, _: &OpenPalette, window, cx| {
                root.open_palette(PaletteMode::All, None, window, cx)
            }))
            .on_action(cx.listener(|root, _: &OpenJump, window, cx| {
                root.open_palette(PaletteMode::Jump, None, window, cx)
            }))
            .on_action(
                cx.listener(|_, _: &ShowShortcuts, window, cx| shortcut_sheet::open(window, cx)),
            )
            .on_action(cx.listener(|root, _: &OpenSettings, window, cx| {
                settings_dialog::open(&root.dashboard, window, cx)
            }))
            .on_action(cx.listener(|_, _: &CheckForUpdates, _, cx| crate::app::update::check(true, cx)))
            .on_action(cx.listener(|_, _: &ToggleAutomaticUpdates, _, cx| {
                crate::app::update::toggle_automatic(cx)
            }))
            .on_action(cx.listener(|root, _: &RestartToUpdate, window, cx| {
                if !root.ask_before_quit_with(window, cx, |_, cx| crate::app::update::restart(cx)) {
                    crate::app::update::restart(cx);
                }
            }))
            .on_action(cx.listener(|root, _: &AddProject, window, cx| {
                projects_dialog::open(&root.dashboard, window, cx)
            }))
            .on_action(cx.listener(Self::import_issues))
            .on_action(cx.listener(|root, _: &UseSystemTheme, _, cx| {
                root.set_theme(ThemeChoice::System, cx)
            }))
            .on_action(
                cx.listener(|root, _: &UseLightTheme, _, cx| {
                    root.set_theme(ThemeChoice::Light, cx)
                }),
            )
            .on_action(
                cx.listener(|root, _: &UseDarkTheme, _, cx| root.set_theme(ThemeChoice::Dark, cx)),
            )
            .on_action(
                cx.listener(|root, _: &UseCompactSize, _, cx| {
                    root.set_ui_size(UiSize::Compact, cx)
                }),
            )
            .on_action(
                cx.listener(|root, _: &UseDefaultSize, _, cx| {
                    root.set_ui_size(UiSize::Default, cx)
                }),
            )
            .on_action(
                cx.listener(|root, _: &UseLargeSize, _, cx| root.set_ui_size(UiSize::Large, cx)),
            )
    }

    pub(super) fn open_palette(
        &mut self,
        mode: PaletteMode,
        scope: Option<PaletteScope>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        PaletteLayer::open(&self.dashboard, mode, scope, window, cx);
    }

    /// The needs-you pill and "N need you": the jump palette on what waits for the user.
    pub(super) fn show_needs_you(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_palette(PaletteMode::Jump, Some(PaletteScope::NeedsYou), window, cx);
    }

    fn set_theme(&mut self, theme: ThemeChoice, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_theme(theme.name(), cx));
    }

    fn set_ui_size(&mut self, size: UiSize, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_ui_size(size.name(), cx));
    }

    /// The issues of the open task's project, else of the project Add task picked last, else the
    /// first project.
    fn import_issues(&mut self, _: &ImportIssues, window: &mut Window, cx: &mut Context<Self>) {
        let open_repo = self
            .current_task(cx)
            .filter(|_| self.dashboard.read(cx).open_task.is_some())
            .and_then(|task| task.repo);
        let dashboard = self.dashboard.read(cx);
        let repo = open_repo
            .or_else(|| task_repo(&dashboard.data.repos, dashboard.last_add_repo.as_deref()));
        issue_import::open(&self.dashboard, repo, window, cx);
    }

    /// Sign in fills the window. `rejected`: the daemon refused the token the app had.
    pub fn show_sign_in(&mut self, rejected: bool, window: &mut Window, cx: &mut Context<Self>) {
        let fake = self
            .source
            .as_deref()
            .is_some_and(|source| matches!(source, crate::app::source::Source::FakeDaemon));
        let view = cx.new(|cx| SignIn::new(fake, rejected, window, cx));
        cx.subscribe_in(&view, window, |root, _, signed: &SignedIn, window, cx| {
            root.signed_in(signed, window, cx)
        })
        .detach();
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        self.sign_in = Some(view);
        cx.notify();
    }

    fn signed_in(&mut self, signed: &SignedIn, window: &mut Window, cx: &mut Context<Self>) {
        let source = signed.0.clone();
        self.source = Some(source.clone());
        self.sign_in = None;
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.reconnect(source, cx));
        let focus = self.tasks.focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// A 401 from the daemon: the app forgets the token it kept and its terminals, then asks
    /// for a token (`rejectToken` of the web). The Electron app's file and the env stay.
    pub(super) fn after_refused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sign_in.is_some() || !self.dashboard.read(cx).data.refused {
            return;
        }
        if let Some(path) = access_file() {
            let _ = natamux_client::config::forget_access(&path);
        }
        TerminalCache::claim(Owner::Workspace, BTreeSet::new(), cx);
        TerminalCache::claim(Owner::Agents, BTreeSet::new(), cx);
        TerminalCache::set_live(BTreeSet::new(), cx);
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.back(cx));
        self.show_sign_in(true, window, cx);
    }
}
