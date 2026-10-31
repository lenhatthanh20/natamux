//! Workflow test T11 (plan 7.5), in the Project settings page of the fake's project: Add stage
//! opens the picker with the focus in its search, the search filters the catalog, the custom
//! stage's id follows its name until the id is typed, a fill of the id by the name does not count
//! as typed, the problems show after the first try, adding selects the stage, and closing gives
//! the focus back to Add stage. The form's buttons have no focus stop, so the tests call what
//! their click calls (`open_custom`, `add_custom`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, FocusHandle, Focusable, TestAppContext, VisualTestContext};

use super::super::editor_catalog::{custom_stage_problems, picker_groups, visible_problems};
use super::super::editor_find::find_stage;
use super::super::pipeline_editor::PipelineEditor;
use super::StagePicker;
use crate::app::test_driver::{activate, is_focused, press, text_of, type_text};
use crate::app::test_views;
use crate::projects::project_settings_page::ProjectSettingsPage;

struct Picking {
    pipeline: Entity<PipelineEditor>,
    picker: Entity<StagePicker>,
    add_stage: FocusHandle,
}

fn focus(view: &Entity<impl Focusable>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    cx.run_until_parked();
}

fn focused(handle: &FocusHandle, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, _| handle.is_focused(window))
}

/// Add stage, pressed with the keyboard on the fake project's page.
fn open_picker(page: &Entity<ProjectSettingsPage>, cx: &mut VisualTestContext) -> Picking {
    let pipeline = page.read_with(cx, |page, _| page.pipeline().expect("read"));
    let (details, list) = page.read_with(cx, |page, _| page.stage_parts().expect("read"));
    let add_stage = list.read_with(cx, |list, _| list.add_stage_focus());
    cx.update(|window, cx| window.focus(&add_stage, cx));
    cx.run_until_parked();
    activate(cx);
    assert!(pipeline.read_with(cx, |pipeline, _| pipeline.picking));
    let picker = details.read_with(cx, |details, _| details.picker().expect("open"));
    Picking {
        pipeline,
        picker,
        add_stage,
    }
}

fn form_texts(picker: &Entity<StagePicker>, cx: &mut VisualTestContext) -> (String, String) {
    let (name, id) = picker.read_with(cx, |picker, _| {
        let form = picker.custom.as_ref().expect("the form is open");
        (form.name.clone(), form.id.clone())
    });
    (text_of(&name, cx), text_of(&id, cx))
}

fn own_id(picker: &Entity<StagePicker>, cx: &mut VisualTestContext) -> Option<String> {
    picker.read_with(cx, |picker, _| {
        picker.custom.as_ref().and_then(|form| form.own_id.clone())
    })
}

fn open_form(picking: &Picking, cx: &mut VisualTestContext) {
    picking
        .picker
        .update_in(cx, |picker, window, cx| picker.open_custom(window, cx));
    cx.run_until_parked();
}

#[gpui::test]
fn t11_add_stage_opens_the_picker_with_the_focus_in_its_search(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_picker(&page, cx);
    let search = picking
        .picker
        .read_with(cx, |picker, _| picker.search.clone());
    assert!(is_focused(&search, cx));
    type_text(cx, "lint");
    assert_eq!(text_of(&search, cx), "lint");
    let shown: Vec<String> = picking.pipeline.read_with(cx, |pipeline, _| {
        picker_groups(pipeline.steps(), "lint")
            .into_iter()
            .flat_map(|(_, entries)| entries)
            .map(|entry| entry.stage.id.clone())
            .collect()
    });
    assert_eq!(shown, ["lint"]);
}

#[gpui::test]
fn t11_the_id_follows_the_name_until_it_is_typed(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_picker(&page, cx);
    open_form(&picking, cx);
    let name = picking
        .picker
        .read_with(cx, |picker, _| picker.custom.as_ref().unwrap().name.clone());
    assert!(is_focused(&name, cx));

    type_text(cx, "Read the plan");
    assert_eq!(
        form_texts(&picking.picker, cx),
        ("Read the plan".to_string(), "read-the-plan".to_string())
    );
    // The id was set by the name, not typed: it keeps following.
    assert_eq!(own_id(&picking.picker, cx), None);

    let id = picking
        .picker
        .read_with(cx, |picker, _| picker.custom.as_ref().unwrap().id.clone());
    focus(&id, cx);
    press(cx, "end");
    type_text(cx, "-2");
    assert_eq!(
        own_id(&picking.picker, cx).as_deref(),
        Some("read-the-plan-2")
    );
    focus(&name, cx);
    press(cx, "end");
    type_text(cx, " again");
    assert_eq!(
        form_texts(&picking.picker, cx),
        (
            "Read the plan again".to_string(),
            "read-the-plan-2".to_string()
        )
    );
}

#[gpui::test]
fn t11_problems_show_after_the_first_try_and_adding_selects_the_stage(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_picker(&page, cx);
    open_form(&picking, cx);
    let steps_before = picking
        .pipeline
        .read_with(cx, |pipeline, _| pipeline.steps().to_vec());
    let shown = |picking: &Picking, cx: &mut VisualTestContext| {
        picking.picker.read_with(cx, |picker, cx| {
            let input = picker.input(cx).expect("the form is open");
            let tried = picker.custom.as_ref().unwrap().tried;
            let steps = picker.pipeline.read(cx).steps();
            visible_problems(&custom_stage_problems(steps, &input), &input, tried)
        })
    };
    // An empty name waits for the first try.
    assert!(shown(&picking, cx).is_empty());
    picking
        .picker
        .update(cx, |picker, cx| picker.add_custom(cx));
    cx.run_until_parked();
    let after_try = shown(&picking, cx);
    assert_eq!(after_try.name.as_deref(), Some("Give the stage a name"));
    let steps_after = picking
        .pipeline
        .read_with(cx, |pipeline, _| pipeline.steps().to_vec());
    assert_eq!(steps_after, steps_before);

    type_text(cx, "Smoke test");
    assert!(shown(&picking, cx).is_empty());
    picking
        .picker
        .update(cx, |picker, cx| picker.add_custom(cx));
    cx.run_until_parked();
    let (picked, picking_now, added) = picking.pipeline.read_with(cx, |pipeline, _| {
        (
            pipeline.selected_id(),
            pipeline.picking,
            find_stage(pipeline.steps(), "smoke-test").map(|stage| stage.name.clone()),
        )
    });
    assert_eq!(picked.as_deref(), Some("smoke-test"));
    assert!(!picking_now);
    // A name the id gives back ("smoke-test" reads "Smoke test") is left out of the stage.
    assert_eq!(added, Some(None));
    // The picker closed, so the focus is back on Add stage.
    assert!(focused(&picking.add_stage, cx));
}

#[gpui::test]
fn t11_closing_gives_the_focus_back_to_add_stage(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_picker(&page, cx);
    assert!(!focused(&picking.add_stage, cx));
    // The close button of the picker.
    picking
        .pipeline
        .update(cx, |pipeline, cx| pipeline.set_picking(false, cx));
    cx.run_until_parked();
    assert!(focused(&picking.add_stage, cx));
    // Add stage again opens a new picker with an empty search.
    activate(cx);
    let (details, _) = page.read_with(cx, |page, _| page.stage_parts().expect("read"));
    let picker = details.read_with(cx, |details, _| details.picker().expect("open"));
    let search = picker.read_with(cx, |picker, _| picker.search.clone());
    assert!(is_focused(&search, cx));
    assert_eq!(text_of(&search, cx), "");
}
