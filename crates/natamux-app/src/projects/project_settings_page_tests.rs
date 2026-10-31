//! Workflow test T15 (plan 7.5), in Settings on the fake's project: Save writes with the version
//! that was read; a file changed since shows Reload and Overwrite, and Overwrite keeps the other
//! client's keys; a broken file asks first; Discard, Reset and Import ask; Export writes the
//! draft; Mod+Enter is ignored while saving; Esc closes the stage picker before Settings; Tab
//! walks the page's stops and wraps. Reload, Overwrite, Reset, Import and Export have no focus
//! stop, so the tests call what their click calls.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use gpui::{AppContext as _, Entity, FocusHandle, TestAppContext, VisualTestContext};
use natamux_proto::stages::catalog::default_pipeline;
use serde_json::{Map, Value};

use super::ProjectSettingsPage;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::settings_rules::{SettingsFocus, SettingsPage};
use crate::app::settings_view::SettingsView;
use crate::app::test_driver::{activate, press, press_now};
use crate::app::test_views::{self, PROJECT};
use crate::components::dialog::DialogLayer;
use crate::stages::pipeline_editor::PipelineEditor;

struct Open {
    settings: Entity<SettingsView>,
    dashboard: Entity<Dashboard>,
    page: Entity<ProjectSettingsPage>,
    pipeline: Entity<PipelineEditor>,
}

/// Settings on the project's page, with the focus on the page, as its Settings button opens it.
fn open_settings(cx: &mut TestAppContext) -> (Open, &mut VisualTestContext) {
    let (settings, dashboard, cx) = test_views::init_app(cx, |dashboard, window, cx| {
        cx.new(|cx| SettingsView::new(dashboard, window, cx))
    });
    dashboard.update(cx, |dashboard, cx| {
        let page = SettingsPage::Project(PROJECT.to_owned());
        dashboard.open_settings(Some(page), SettingsFocus::Page, cx)
    });
    settings.update_in(cx, |settings, window, cx| {
        settings.show(SettingsFocus::Page, window, cx)
    });
    cx.run_until_parked();
    let page = settings.read_with(cx, |settings, _| {
        settings.project_page(PROJECT).expect("made")
    });
    let pipeline = page.read_with(cx, |page, _| page.pipeline().expect("read"));
    let open = Open {
        settings,
        dashboard,
        page,
        pipeline,
    };
    (open, cx)
}

/// The file of the fake now: its version and its keys.
fn file() -> (Option<String>, Map<String, Value>) {
    let loaded = fake_daemon::shared().load_settings(PROJECT).unwrap();
    let text = loaded.text.unwrap_or_default();
    let raw = serde_json::from_str(&text).unwrap_or_default();
    (loaded.version, raw)
}

fn send_backs_of(raw: &Map<String, Value>) -> Option<u64> {
    raw.get("agents")?.get("maxSendBacks")?.as_u64()
}

fn edit(open: &Open, most: usize, cx: &mut VisualTestContext) {
    open.pipeline.update(cx, |pipeline, cx| {
        pipeline.set_max_send_backs(Some(most), cx)
    });
    cx.run_until_parked();
}

fn dialog_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| DialogLayer::is_open(cx))
}

fn focused(handle: &FocusHandle, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

#[gpui::test]
fn t15_save_writes_with_the_version_that_was_read(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    assert_eq!(file().0.as_deref(), Some("v1"));
    edit(&open, 3, cx);
    assert!(open.page.read_with(cx, |page, cx| page.dirty(cx)));
    press(cx, "mod-enter");
    let (version, raw) = file();
    assert_eq!(version.as_deref(), Some("v2"));
    assert_eq!(send_backs_of(&raw), Some(3));
    let (dirty, conflict, base) = open.page.read_with(cx, |page, cx| {
        let base = page.base.as_ref().and_then(|base| base.version.clone());
        (page.dirty(cx), page.conflict, base)
    });
    assert!(!dirty);
    assert!(!conflict);
    assert_eq!(base.as_deref(), Some("v2"));
}

#[gpui::test]
fn t15_mod_enter_is_ignored_while_saving(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    edit(&open, 3, cx);
    press_now(cx, "mod-enter");
    assert!(open.page.read_with(cx, |page, _| page.saving));
    // A second save would send the old version and get the 409.
    press_now(cx, "mod-enter");
    press_now(cx, "escape");
    cx.run_until_parked();
    assert_eq!(file().0.as_deref(), Some("v2"));
    assert!(!open.page.read_with(cx, |page, _| page.conflict));
    assert!(
        open.dashboard
            .read_with(cx, |dashboard, _| dashboard.settings.is_some())
    );
}

#[gpui::test]
fn t15_a_changed_file_offers_reload_which_drops_the_edits(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    edit(&open, 3, cx);
    fake_daemon::shared().move_settings_version(PROJECT);
    press(cx, "mod-enter");
    assert!(open.page.read_with(cx, |page, _| page.conflict));
    let (version, raw) = file();
    assert_eq!(version.as_deref(), Some("v2"));
    assert_eq!(send_backs_of(&raw), None);

    open.page.update(cx, |page, cx| page.reload(cx));
    cx.run_until_parked();
    let (conflict, dirty, most) = open.page.read_with(cx, |page, cx| {
        let most = page.pipeline().unwrap().read(cx).max_send_backs();
        (page.conflict, page.dirty(cx), most)
    });
    assert!(!conflict);
    assert!(!dirty);
    assert_eq!(most, None);
    assert_eq!(file().0.as_deref(), Some("v2"));
}

#[gpui::test]
fn t15_overwrite_keeps_the_other_clients_keys(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    edit(&open, 3, cx);
    // Another client turns off the PR link.
    let (_, mut raw) = file();
    raw["tracker"]["linkInPr"] = Value::Bool(false);
    fake_daemon::shared()
        .save_settings(PROJECT, raw, None)
        .unwrap();
    press(cx, "mod-enter");
    assert!(open.page.read_with(cx, |page, _| page.conflict));

    open.page.update(cx, |page, cx| page.overwrite(cx));
    cx.run_until_parked();
    let (version, raw) = file();
    assert_eq!(version.as_deref(), Some("v3"));
    assert_eq!(send_backs_of(&raw), Some(3));
    assert_eq!(raw["tracker"]["linkInPr"], Value::Bool(false));
    assert!(!open.page.read_with(cx, |page, _| page.conflict));
}

#[gpui::test]
fn t15_a_broken_file_asks_before_it_is_replaced(cx: &mut TestAppContext) {
    fake_daemon::shared().break_settings(PROJECT, "line 3: expected a value");
    let (open, cx) = open_settings(cx);
    let broken = open.page.read_with(cx, |page, _| {
        page.base.as_ref().and_then(|base| base.broken.clone())
    });
    assert_eq!(broken.as_deref(), Some("line 3: expected a value"));
    press(cx, "mod-enter");
    assert!(dialog_open(cx), "no question before the file is replaced");
    assert_eq!(file().0.as_deref(), Some("v1"));
    // Mod+Enter in the question answers Save anyway.
    press(cx, "mod-enter");
    assert!(!dialog_open(cx));
    assert_eq!(file().0.as_deref(), Some("v2"));
}

#[gpui::test]
fn t15_discard_asks_then_drops_the_draft(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    edit(&open, 3, cx);
    let discard = open.page.read_with(cx, |page, _| page.discard.clone());
    cx.update(|window, cx| window.focus(&discard, cx));
    cx.run_until_parked();
    activate(cx);
    assert!(dialog_open(cx));
    press(cx, "mod-enter");
    assert!(!dialog_open(cx));
    let page = open.settings.read_with(cx, |settings, _| {
        settings.project_page(PROJECT).expect("made again")
    });
    assert_ne!(page.entity_id(), open.page.entity_id());
    assert!(!page.read_with(cx, |page, cx| page.dirty(cx)));
    assert_eq!(file().0.as_deref(), Some("v1"));
}

#[gpui::test]
fn t15_reset_asks_then_puts_the_default_stages_back(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    open.page
        .update_in(cx, |page, window, cx| page.reset(window, cx));
    cx.run_until_parked();
    assert!(dialog_open(cx));
    press(cx, "mod-enter");
    assert!(!dialog_open(cx));
    let steps = open
        .pipeline
        .read_with(cx, |pipeline, _| pipeline.steps().to_vec());
    assert_eq!(steps, default_pipeline());
    assert!(open.page.read_with(cx, |page, cx| page.dirty(cx)));
}

fn scratch_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("natamux-t15-{}-{name}", std::process::id()))
}

#[gpui::test]
fn t15_import_over_changes_asks_then_takes_the_file(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    edit(&open, 3, cx);
    let path = scratch_file("import.json");
    let text = "{ \"agents\": { \"maxSendBacks\": 7 } }\n";
    std::fs::write(&path, text).unwrap();
    open.page
        .update_in(cx, |page, window, cx| page.import(window, cx));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_paths());
    let picked = path.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![picked]));
    cx.run_until_parked();
    assert!(
        dialog_open(cx),
        "no question before the changes are replaced"
    );
    press(cx, "mod-enter");
    let (draft, most) = open.pipeline.read_with(cx, |pipeline, _| {
        (pipeline.text().to_owned(), pipeline.max_send_backs())
    });
    assert_eq!(draft, text);
    assert_eq!(most, Some(7));
    assert_eq!(file().0.as_deref(), Some("v1"));
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn t15_export_writes_the_draft(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    edit(&open, 4, cx);
    let path = scratch_file("export.json");
    open.page
        .update_in(cx, |page, window, cx| page.export(window, cx));
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    let picked = path.clone();
    cx.simulate_new_path_selection(move |_| Some(picked));
    cx.run_until_parked();
    let written = std::fs::read_to_string(&path).unwrap();
    let raw: Map<String, Value> = serde_json::from_str(&written).unwrap();
    assert_eq!(send_backs_of(&raw), Some(4));
    assert!(written.ends_with('\n'));
    // Export changes no file of the project and keeps the draft.
    assert_eq!(file().0.as_deref(), Some("v1"));
    assert!(open.page.read_with(cx, |page, cx| page.dirty(cx)));
    std::fs::remove_file(&path).ok();
}

#[gpui::test]
fn t15_esc_closes_the_stage_picker_before_settings(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    open.pipeline
        .update(cx, |pipeline, cx| pipeline.set_picking(true, cx));
    cx.run_until_parked();
    press(cx, "escape");
    assert!(!open.pipeline.read_with(cx, |pipeline, _| pipeline.picking));
    assert!(
        open.dashboard
            .read_with(cx, |dashboard, _| dashboard.settings.is_some())
    );
    press(cx, "escape");
    assert!(
        open.dashboard
            .read_with(cx, |dashboard, _| dashboard.settings.is_none())
    );
}

#[gpui::test]
fn t15_tab_walks_the_stops_of_the_page_and_wraps(cx: &mut TestAppContext) {
    let (open, cx) = open_settings(cx);
    let ring = open.page.read_with(cx, |page, cx| page.tab_ring(cx));
    assert!(ring.len() > 4, "{} stops", ring.len());
    cx.update(|window, cx| window.focus(&ring[0], cx));
    cx.run_until_parked();
    let mut wrong = Vec::new();
    for (step, want) in ring.iter().enumerate().skip(1) {
        press(cx, "tab");
        if !focused(want, cx) {
            wrong.push(format!("Tab {step} of {} missed its stop", ring.len()));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
    // After Save, Tab goes back to the search of Settings, outside the page.
    press(cx, "tab");
    let in_page = cx.update(|window, _| ring.iter().any(|stop| stop.is_focused(window)));
    assert!(!in_page);
    press(cx, "shift-tab");
    assert!(focused(ring.last().unwrap(), cx));
}
