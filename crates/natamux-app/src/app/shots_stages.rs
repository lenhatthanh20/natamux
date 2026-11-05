//! Screen 5's `--shot` states of Project settings: the project's page of Settings opens, then one
//! step puts it in the state once its settings are read.

use std::time::Duration;

use gpui::{AsyncApp, WindowHandle};
use natamux_proto::stages::schema::Step;

use super::fake_daemon;
use super::options::Shot;
use super::settings_rules::SettingsPage;
use super::window::Root;
use crate::projects::settings_tab_rules::SettingsTab;
use crate::stages::add_stage_dialog;
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
            | Shot::ProjectSettingsStageSheet
            | Shot::ProjectSettingsStageSheetAdvanced
            | Shot::ProjectSettingsAddStage
            | Shot::ProjectSettingsBranch
            | Shot::ProjectSettingsProblems
            | Shot::ProjectSettingsConflict
    )
}

/// Opens the page, waits for its read, then takes the step of `shot` on the Stages tab.
pub async fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut AsyncApp) {
    let opened = handle.update(cx, |root, window, cx| {
        root.show_settings(SettingsPage::Project(PROJECT.to_owned()), window, cx)
    });
    let Ok(Some(page)) = opened else {
        return;
    };
    cx.background_executor().timer(READ).await;
    handle
        .update(cx, |_, window, cx| {
            page.update(cx, |page, cx| page.pick_tab(SettingsTab::Stages, cx));
            let Some(editor) = page.read(cx).pipeline() else {
                return;
            };
            let steps: Vec<Step> = editor.read(cx).steps().to_vec();
            match shot {
                // A row or node opens its sheet: the stage sheet on Basic.
                Shot::ProjectSettingsStageSheet => {
                    editor.update(cx, |editor, cx| editor.select(AGENT_STAGE, cx));
                }
                // A stage with a problem in a command field opens on Advanced.
                Shot::ProjectSettingsStageSheetAdvanced => {
                    let next = add_catalog_stage(&steps, NEEDS_COMMAND, None);
                    editor.update(cx, |editor, cx| {
                        editor.edit(next, cx);
                        editor.select(NEEDS_COMMAND, cx);
                    });
                }
                Shot::ProjectSettingsBranch => {
                    editor.update(cx, |editor, cx| editor.select(BRANCH, cx));
                }
                Shot::ProjectSettingsProblems => {
                    let next = add_catalog_stage(&steps, NEEDS_COMMAND, None);
                    editor.update(cx, |editor, cx| editor.edit(next, cx));
                    // The problems sit in the Stages group's footnote, under the flowchart.
                    page.update(cx, |page, cx| page.show_details(cx));
                }
                Shot::ProjectSettingsAddStage => add_stage_dialog::open(&editor, window, cx),
                Shot::ProjectSettingsConflict => {
                    // Another client saves first, so this Save gets the 409.
                    fake_daemon::shared().move_settings_version(PROJECT);
                    let next = set_enabled(&steps, TURNED_OFF, false);
                    editor.update(cx, |editor, cx| editor.edit(next, cx));
                    page.update(cx, |page, cx| page.save(window, cx));
                }
                _ => {}
            }
        })
        .ok();
}
