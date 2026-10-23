//! Screen 7's `--shot` states: the palettes, the shortcut sheet, the account menu, the settings
//! and font dialogs, the project dialogs and tabs, and Sign in.

use std::time::Duration;

use gpui::{AsyncApp, Entity, WindowHandle, point, px};

use super::account_menu::open_account_menu;
use super::fake_daemon;
use super::options::Shot;
use super::window::Root;
use super::{settings_dialog, shortcut_sheet};
use crate::palette::palette_layer::PaletteLayer;
use crate::palette::palette_rows::{PaletteMode, PaletteScope};
use crate::projects::project_settings_dialog::ProjectSettingsDialog;
use crate::projects::settings_tab_rules::SettingsTab;
use crate::projects::{issue_import, project_settings_dialog, projects_dialog};
use crate::terminals::terminal_font_dialog;

/// The project of the settings shots: it has a `.natamux.json`.
const PROJECT: &str = "natamux";
/// The task whose terminals sit under the font dialog.
const RUNNING: &str = "T-0146";
/// The palette queries: one that finds commands, one that finds nothing.
const QUERY: &str = "stage";
const NO_MATCH: &str = "qqzz";
/// JSON that does not parse, as the reference's parse error.
const BROKEN_JSON: &str = "{ \"stages\": [";
/// The settings are read and drawn within this.
const READ: Duration = Duration::from_millis(800);
/// The account menu opens above the status bar's last item.
const MENU_FROM_RIGHT: f32 = 8.;
const MENU_FROM_BOTTOM: f32 = 30.;

pub fn is_screen7_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::Palette
            | Shot::PaletteQuery
            | Shot::PaletteNoMatch
            | Shot::PaletteJump
            | Shot::Shortcuts
            | Shot::AccountMenu
            | Shot::TerminalFont
            | Shot::SettingsDialog
            | Shot::Projects
            | Shot::ProjectSettings
            | Shot::ProjectSettingsTabIssues
            | Shot::ProjectSettingsTabJson
            | Shot::ProjectSettingsJsonError
            | Shot::ImportIssues
            | Shot::SignIn
            | Shot::SignInError
    )
}

/// The fake's data of the shot, before the window watches it.
pub fn prepare(shot: Shot) {
    fake_daemon::shared().set_gh_missing(shot == Shot::ImportIssues);
}

pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    if matches!(shot, Shot::TerminalFont) {
        handle
            .update(cx, |root, _, cx| {
                root.dashboard()
                    .update(cx, |dashboard, cx| dashboard.open(RUNNING, cx))
            })
            .ok();
        cx.background_executor().timer(READ).await;
    }
    let Ok(dialog) = handle.update(cx, |root, window, cx| enter(shot, root, window, cx)) else {
        return;
    };
    let tab = match shot {
        Shot::ProjectSettingsTabIssues => Some(SettingsTab::Issues),
        Shot::ProjectSettingsTabJson | Shot::ProjectSettingsJsonError => Some(SettingsTab::Json),
        _ => None,
    };
    if let (Some(tab), Some(dialog)) = (tab, dialog) {
        cx.background_executor().timer(READ).await;
        handle
            .update(cx, |_, _, cx| pick_tab(shot, tab, &dialog, cx))
            .ok();
    }
}

/// Puts the window in the state; Project settings gives its dialog for the tab step.
fn enter(
    shot: Shot,
    root: &mut Root,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Root>,
) -> Option<Entity<ProjectSettingsDialog>> {
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
        Shot::AccountMenu => {
            let size = window.viewport_size();
            let at = point(
                size.width - px(MENU_FROM_RIGHT),
                size.height - px(MENU_FROM_BOTTOM),
            );
            open_account_menu(&dashboard, at, window, cx);
        }
        Shot::TerminalFont => terminal_font_dialog::open(&dashboard, window, cx),
        Shot::SettingsDialog => settings_dialog::open(&dashboard, window, cx),
        Shot::Projects => projects_dialog::open(&dashboard, window, cx),
        Shot::ProjectSettings
        | Shot::ProjectSettingsTabIssues
        | Shot::ProjectSettingsTabJson
        | Shot::ProjectSettingsJsonError => {
            return Some(project_settings_dialog::open(
                &dashboard, PROJECT, window, cx,
            ));
        }
        Shot::ImportIssues => issue_import::open(&dashboard, Some(PROJECT.to_owned()), window, cx),
        Shot::SignIn => root.show_sign_in(false, window, cx),
        Shot::SignInError => root.show_sign_in(true, window, cx),
        _ => {}
    }
    None
}

fn pick_tab(
    shot: Shot,
    tab: SettingsTab,
    dialog: &Entity<ProjectSettingsDialog>,
    cx: &mut gpui::Context<Root>,
) {
    if shot == Shot::ProjectSettingsJsonError
        && let Some(editor) = dialog.read(cx).pipeline()
    {
        editor.update(cx, |editor, cx| editor.set_text(BROKEN_JSON, cx));
    }
    dialog.update(cx, |dialog, cx| dialog.pick_tab(tab, cx));
}
