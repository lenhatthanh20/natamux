//! Screen 7's `--shot` states: the palettes, the shortcut sheet, the account menu, the Settings
//! page (T-0174) with its app pages, a search and a project's tabs, the Projects dialog, Sign in,
//! the app update in the status bar and in Settings (T-0214), the repo trust dialog, and What's
//! new (T-0301).

use std::time::Duration;

use gpui::{AsyncApp, Entity, WindowHandle, point, px};
use natamux_proto::agents::custom::CustomAgent;

use super::account_menu::open_account_menu;
use super::fake_daemon;
use super::options::Shot;
use super::settings_agents::open_agent_sheet;
use super::settings_rules::SettingsPage;
use super::shortcut_sheet;
use super::update::show_for_shot;
use super::update_state::UpdateState;
use super::whats_new;
use super::window::Root;
use crate::palette::palette_layer::PaletteLayer;
use crate::palette::palette_rows::{PaletteMode, PaletteScope};
use crate::projects::project_settings_page::ProjectSettingsPage;
use crate::projects::repo_trust_dialog::{review, rules::repo_name};
use crate::projects::settings_tab_rules::SettingsTab;
use crate::projects::{issue_import, projects_dialog};

/// The project of the settings shots: it has a `.natamux.json`.
const PROJECT: &str = "natamux";
/// The palette queries: one that finds commands, one that finds nothing.
const QUERY: &str = "stage";
/// The Settings search: it finds the terminal font rows.
const SETTINGS_QUERY: &str = "font";
const NO_MATCH: &str = "qqzz";
/// JSON that does not parse, as the reference's parse error.
const BROKEN_JSON: &str = "{ \"stages\": [";
/// The settings are read and drawn within this.
const READ: Duration = Duration::from_millis(800);
/// The account menu opens under the title bar's last button.
const MENU_FROM_RIGHT: f32 = 12.;
const MENU_FROM_TOP: f32 = 30.;
/// The update shots: a version after the running one, and a download under way.
const NEXT_VERSION: &str = "9.9.9";
const DOWNLOADED: u64 = 42 * 1024 * 1024;
/// Fixed notes for the What's new shot, in the form of `docs/releases/*.md`.
const SHOT_NOTES: &str = "This version adds a What's new dialog. If you have v0.1.2 or later, \
the app updates itself to this version.

Guide: https://natamux.github.io/docs/

## Desktop app

- After an update, the app shows what changed in the new version.
- **What's New** in the menu and in Settings opens the notes again.

## Fixes

- Fix: a long release note scrolls inside the dialog.

## Download

| System | File |
|---|---|
| macOS, Apple silicon | `Natamux-mac-arm64.dmg` |
";

pub fn is_screen7_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::Palette
            | Shot::PaletteQuery
            | Shot::PaletteNoMatch
            | Shot::PaletteJump
            | Shot::Shortcuts
            | Shot::AccountMenu
            | Shot::Settings
            | Shot::SettingsGeneral
            | Shot::SettingsAppearance
            | Shot::SettingsAgents
            | Shot::SettingsAgentSheet
            | Shot::SettingsSearch
            | Shot::Projects
            | Shot::ProjectSettings
            | Shot::ProjectSettingsTabGeneral
            | Shot::ProjectSettingsTabJson
            | Shot::ProjectSettingsJsonError
            | Shot::ProjectSettingsTrust
            | Shot::ImportIssues
            | Shot::SignIn
            | Shot::SignInError
            | Shot::UpdateDownloading
            | Shot::UpdateReady
            | Shot::UpdateFailed
            | Shot::SettingsUpdateReady
            | Shot::RepoTrustDialog
            | Shot::WhatsNew
    )
}

/// The fake's data of the shot, before the window watches it.
pub fn prepare(shot: Shot) {
    fake_daemon::shared().set_gh_missing(shot == Shot::ImportIssues);
    if matches!(shot, Shot::RepoTrustDialog | Shot::ProjectSettingsTrust) {
        fake_daemon::shared().distrust_repo(PROJECT);
    }
}

pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let Ok(page) = handle.update(cx, |root, window, cx| enter(shot, root, window, cx)) else {
        return;
    };
    if shot == Shot::SettingsAgentSheet {
        // The agents page takes its draft when it draws.
        cx.background_executor().timer(READ).await;
        handle
            .update(cx, |root, window, cx| {
                let view = root.settings_view().clone();
                open_agent_sheet(&view, Some(0), window, cx)
            })
            .ok();
    }
    let tab = match shot {
        Shot::ProjectSettingsTabGeneral | Shot::ProjectSettingsTrust => Some(SettingsTab::General),
        Shot::ProjectSettingsTabJson | Shot::ProjectSettingsJsonError => Some(SettingsTab::Json),
        _ => None,
    };
    if let (Some(tab), Some(page)) = (tab, page) {
        cx.background_executor().timer(READ).await;
        handle
            .update(cx, |_, _, cx| pick_tab(shot, tab, &page, cx))
            .ok();
    }
}

/// Puts the window in the state; Project settings gives its page for the tab step.
fn enter(
    shot: Shot,
    root: &mut Root,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Root>,
) -> Option<Entity<ProjectSettingsPage>> {
    let dashboard = root.dashboard().clone();
    match shot {
        Shot::Palette => PaletteLayer::open(&dashboard, PaletteMode::All, None, window, cx),
        Shot::PaletteQuery | Shot::PaletteNoMatch => {
            PaletteLayer::open(&dashboard, PaletteMode::All, None, window, cx);
            let query = if shot == Shot::PaletteQuery {
                QUERY
            } else {
                NO_MATCH
            };
            PaletteLayer::set_query(query, window, cx);
        }
        Shot::PaletteJump => PaletteLayer::open(
            &dashboard,
            PaletteMode::Jump,
            Some(PaletteScope::NeedsYou),
            window,
            cx,
        ),
        Shot::Shortcuts => shortcut_sheet::open(window, cx),
        Shot::WhatsNew => whats_new::open_notes(SHOT_NOTES, window, cx),
        Shot::AccountMenu => {
            let width = window.viewport_size().width;
            let at = point(width - px(MENU_FROM_RIGHT), px(MENU_FROM_TOP));
            open_account_menu(&dashboard, at, window, cx);
        }
        Shot::Settings => {
            root.show_settings(SettingsPage::General, window, cx);
        }
        Shot::SettingsAppearance => {
            root.show_settings(SettingsPage::Appearance, window, cx);
        }
        Shot::SettingsGeneral => {
            root.show_settings(SettingsPage::General, window, cx);
        }
        Shot::SettingsAgents | Shot::SettingsAgentSheet => {
            // The fake has none, so other shots' agent menus stay as they were.
            let sample = CustomAgent {
                id: "claude-qwen".into(),
                base: "claude-code".into(),
                command: "cq".into(),
            };
            dashboard.update(cx, |dashboard, cx| {
                dashboard.use_custom_agents(vec![sample], cx)
            });
            root.show_settings(SettingsPage::Agents, window, cx);
        }
        Shot::SettingsSearch => {
            root.show_settings(SettingsPage::Appearance, window, cx);
            root.settings_view().update(cx, |settings, cx| {
                settings.set_query(SETTINGS_QUERY, window, cx)
            });
        }
        Shot::Projects => projects_dialog::open(&dashboard, window, cx),
        Shot::ProjectSettings
        | Shot::ProjectSettingsTabGeneral
        | Shot::ProjectSettingsTabJson
        | Shot::ProjectSettingsJsonError
        | Shot::ProjectSettingsTrust => {
            return root.show_settings(SettingsPage::Project(PROJECT.to_owned()), window, cx);
        }
        Shot::ImportIssues => issue_import::open(&dashboard, Some(PROJECT.to_owned()), window, cx),
        Shot::SignIn => root.show_sign_in(false, window, cx),
        Shot::SignInError => root.show_sign_in(true, window, cx),
        Shot::UpdateDownloading => show_for_shot(downloading(), DOWNLOADED, cx),
        Shot::UpdateReady => show_for_shot(ready(), 0, cx),
        Shot::UpdateFailed => show_for_shot(
            UpdateState::Failed {
                message: "The latest release could not be reached".into(),
            },
            0,
            cx,
        ),
        Shot::SettingsUpdateReady => {
            show_for_shot(ready(), 0, cx);
            root.show_settings(SettingsPage::General, window, cx);
        }
        Shot::RepoTrustDialog => {
            let name = repo_name(&dashboard.read(cx).data.repos, PROJECT);
            if let Some(source) = dashboard.read(cx).source().cloned() {
                review(&source, PROJECT, &name, None, window, cx);
            }
        }
        _ => {}
    }
    None
}

fn pick_tab(
    shot: Shot,
    tab: SettingsTab,
    page: &Entity<ProjectSettingsPage>,
    cx: &mut gpui::Context<Root>,
) {
    if shot == Shot::ProjectSettingsJsonError
        && let Some(editor) = page.read(cx).pipeline()
    {
        editor.update(cx, |editor, cx| editor.set_text(BROKEN_JSON, cx));
    }
    page.update(cx, |page, cx| page.pick_tab(tab, cx));
}

fn downloading() -> UpdateState {
    UpdateState::Downloading {
        version: NEXT_VERSION.into(),
        manual: true,
    }
}

fn ready() -> UpdateState {
    UpdateState::Ready {
        version: NEXT_VERSION.into(),
    }
}
