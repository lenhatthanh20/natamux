//! Workflow test T11 (plan 7.5), in the Project settings page of the fake's project, on main's
//! Add menu (T-0285, T-0313): Built-in stage opens the Add stage sheet with the focus in its
//! search, the search filters the catalog, Enter adds the active stage and opens its sheet, and
//! closing gives the focus back to Add. Esc in the search does not close the sheet today (see
//! the test). The custom stage part of T11 is in `custom_stage_dialog_tests.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{DismissEvent, Entity, FocusHandle, TestAppContext, VisualTestContext};

use super::super::add_stage_dialog::AddStageDialog;
use super::super::editor_catalog::picker_groups;
use super::super::pipeline_editor::PipelineEditor;
use super::StagePicker;
use crate::app::test_driver::{activate, click_on, is_focused, press, text_of, type_text};
use crate::app::test_views;
use crate::components::dialog::DialogLayer;
use crate::projects::project_settings_page::ProjectSettingsPage;

pub(in crate::stages) struct Picking {
    pub pipeline: Entity<PipelineEditor>,
    pub add_stage: FocusHandle,
}

pub(in crate::stages) fn focused(handle: &FocusHandle, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

/// The Add button pressed with the keyboard on the fake project's page, then the menu item.
pub(in crate::stages) fn open_from_menu(
    page: &Entity<ProjectSettingsPage>,
    item: &'static str,
    cx: &mut VisualTestContext,
) -> Picking {
    let pipeline = page.read_with(cx, |page, _| page.pipeline().expect("read"));
    let (_, list) = page.read_with(cx, |page, _| page.stage_parts().expect("read"));
    let add_stage = list.read_with(cx, |list, _| list.add_stage_focus());
    cx.update(|window, cx| window.focus(&add_stage, cx));
    cx.run_until_parked();
    activate(cx);
    cx.run_until_parked();
    click_on(cx, item);
    cx.run_until_parked();
    Picking {
        pipeline,
        add_stage,
    }
}

fn dialog(cx: &mut VisualTestContext) -> Entity<AddStageDialog> {
    let top = cx
        .update(|_, cx| DialogLayer::top_view(cx))
        .expect("a dialog is open");
    let Ok(dialog) = top.downcast::<AddStageDialog>() else {
        panic!("the top dialog is not the Add stage sheet");
    };
    dialog
}

fn picker(cx: &mut VisualTestContext) -> Entity<StagePicker> {
    let dialog = dialog(cx);
    dialog.read_with(cx, |dialog, _| dialog.picker())
}

const BUILT_IN: &str = "MENU_ITEM-Built-in stage\u{2026}";

#[gpui::test]
fn t11_add_stage_opens_the_picker_with_the_focus_in_its_search(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_from_menu(&page, BUILT_IN, cx);
    let picker = picker(cx);
    let search = picker.read_with(cx, |picker, _| picker.search.clone());
    assert!(is_focused(&search, cx));
    // With no search, the picker draws every stage of the catalog not yet in the pipeline.
    let all: Vec<String> = picking.pipeline.read_with(cx, |pipeline, _| {
        picker_groups(pipeline.steps(), "")
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .map(|entry| entry.stage.id.clone())
            .collect()
    });
    let drawn = |cx: &mut VisualTestContext| picker.read_with(cx, |picker, _| picker.drawn_rows());
    assert!(all.len() > 1, "{all:?}");
    assert_eq!(drawn(cx), all);
    type_text(cx, "lint");
    assert_eq!(text_of(&search, cx), "lint");
    assert_eq!(drawn(cx), ["lint"]);
}

#[gpui::test]
fn t11_closing_gives_the_focus_back_to_add_stage(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_from_menu(&page, BUILT_IN, cx);
    assert!(!focused(&picking.add_stage, cx));
    let search = picker(cx).read_with(cx, |picker, _| picker.search.clone());
    type_text(cx, "lint");
    assert_eq!(text_of(&search, cx), "lint");
    // Esc in the search leaves the sheet open: the sheet's focus handle is also the Cancel
    // button's, so the dialog layer does not see the search as inside the sheet (main's code,
    // kept; see "Bugs found" in the log).
    press(cx, "escape");
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
    // What the Cancel button's click does.
    dialog(cx).update(cx, |_, cx| cx.emit(DismissEvent));
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| DialogLayer::is_open(cx)));
    assert!(focused(&picking.add_stage, cx));
    // Add again opens a new picker with an empty search.
    activate(cx);
    cx.run_until_parked();
    click_on(cx, BUILT_IN);
    cx.run_until_parked();
    let search = picker(cx).read_with(cx, |picker, _| picker.search.clone());
    assert!(is_focused(&search, cx));
    assert_eq!(text_of(&search, cx), "");
}

#[gpui::test]
fn t11_enter_adds_the_active_stage_and_opens_its_sheet(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_from_menu(&page, BUILT_IN, cx);
    type_text(cx, "lint");
    press(cx, "enter");
    cx.run_until_parked();
    let picked = picking
        .pipeline
        .read_with(cx, |pipeline, _| pipeline.selected_id());
    assert_eq!(picked.as_deref(), Some("lint"));
    let top = cx.update(|_, cx| DialogLayer::top_view(cx));
    let sheet = top.and_then(|top| top.downcast::<super::super::stage_sheet::StageSheet>().ok());
    assert!(
        sheet.is_some(),
        "the stage sheet of the new stage did not open"
    );
}
