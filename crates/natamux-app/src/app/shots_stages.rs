//! Screen 5's `--shot` states of Project settings: the project's page of Settings opens, then one
//! step puts it in the state once its settings are read.

use std::time::Duration;

use gpui::{AsyncApp, WindowHandle};
use natamux_proto::stages::schema::Step;

use super::fake_daemon;
use super::options::Shot;
use super::settings_rules::SettingsPage;
use super::window::Root;
use crate::stages::editor_catalog::add_catalog_stage;
use crate::stages::editor_fields::set_enabled;

/// The project of the shots: it has a `.natamux.json`.
pub(crate) const PROJECT: &str = "natamux";
/// "Reproduce the bug": an agent stage on its own agent with a prompt of its own.
const AGENT_STAGE: &str = "reproduce";
/// "Bug or feature".
const BRANCH: &str = "kind";
/// "Lint and types", a testing stage that needs the project's command: added without one, it
/// is a problem.
const NEEDS_COMMAND: &str = "lint";
/// A stage turned off, so the conflict shot has an edit to save.
const TURNED_OFF: &str = "security";
/// The settings are read and drawn within this.
pub(crate) const READ: Duration = Duration::from_millis(800);

pub fn is_settings_shot(shot: Shot) -> bool {
    matches!(
        shot,
        Shot::ProjectSettingsTabStages
            | Shot::ProjectSettingsAgentStage
            | Shot::ProjectSettingsStagePicker
            | Shot::ProjectSettingsBranch
            | Shot::ProjectSettingsProblems
            | Shot::ProjectSettingsConflict
    )
}

/// Opens the page, waits for its read, then takes the step of `shot`.
pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let opened = handle.update(cx, |root, window, cx| {
        root.show_settings(SettingsPage::Project(PROJECT.to_owned()), window, cx)
    });
    let Ok(Some(dialog)) = opened else {
        return;
    };
    cx.background_executor().timer(READ).await;
    handle
        .update(cx, |_, window, cx| {
            let Some(editor) = dialog.read(cx).pipeline() else {
                return;
            };
            let steps: Vec<Step> = editor.read(cx).steps().to_vec();
            match shot {
                Shot::ProjectSettingsAgentStage => {
                    editor.update(cx, |editor, cx| editor.select(AGENT_STAGE, cx));
                }
                Shot::ProjectSettingsStagePicker => {
                    editor.update(cx, |editor, cx| editor.set_picking(true, cx));
                }
                Shot::ProjectSettingsBranch => {
                    editor.update(cx, |editor, cx| editor.select(BRANCH, cx));
                }
                Shot::ProjectSettingsProblems => {
                    let next = add_catalog_stage(&steps, NEEDS_COMMAND, None);
                    editor.update(cx, |editor, cx| {
                        editor.edit(next, cx);
                        editor.select(NEEDS_COMMAND, cx);
                    });
                }
                Shot::ProjectSettingsConflict => {
                    // Another client saves first, so this Save gets the 409.
                    fake_daemon::shared().move_settings_version(PROJECT);
                    let next = set_enabled(&steps, TURNED_OFF, false);
                    editor.update(cx, |editor, cx| editor.edit(next, cx));
                    dialog.update(cx, |dialog, cx| dialog.save(window, cx));
                }
                _ => {}
            }
        })
        .ok();
    // The list and the details sit under the preview: the shots show them.
    if !matches!(
        shot,
        Shot::ProjectSettingsTabStages | Shot::ProjectSettingsConflict
    ) {
        cx.background_executor().timer(READ).await;
        handle
            .update(cx, |_, _, cx| {
                dialog.update(cx, |dialog, cx| dialog.show_details(cx))
            })
            .ok();
    }
}
