//! Workflow test T11 (plan 7.5), the custom stage part, on main's Custom stage sheet (T-0285):
//! the Add menu opens it with the focus in Name, the id follows the name until the id is typed,
//! a fill of the id by the name does not count as typed, the problems show after the first try,
//! and adding selects the stage and opens its sheet.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};

use super::super::editor_catalog::{custom_stage_problems, visible_problems};
use super::super::editor_find::find_stage;
use super::super::stage_picker::tests::open_from_menu;
use super::super::stage_sheet::StageSheet;
use super::CustomStageDialog;
use crate::app::test_driver::{is_focused, press, text_of, type_text};
use crate::app::test_views;
use crate::components::dialog::DialogLayer;

const CUSTOM: &str = "MENU_ITEM-Custom stage\u{2026}";

fn dialog(cx: &mut VisualTestContext) -> Entity<CustomStageDialog> {
    let top = cx
        .update(|_, cx| DialogLayer::top_view(cx))
        .expect("a dialog is open");
    let Ok(dialog) = top.downcast::<CustomStageDialog>() else {
        panic!("the top dialog is not the Custom stage sheet");
    };
    dialog
}

fn focus(view: &Entity<impl Focusable>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    cx.run_until_parked();
}

fn form_texts(dialog: &Entity<CustomStageDialog>, cx: &mut VisualTestContext) -> (String, String) {
    let (name, id) = dialog.read_with(cx, |dialog, _| (dialog.name.clone(), dialog.id.clone()));
    (text_of(&name, cx), text_of(&id, cx))
}

fn own_id(dialog: &Entity<CustomStageDialog>, cx: &mut VisualTestContext) -> Option<String> {
    dialog.read_with(cx, |dialog, _| dialog.own_id.clone())
}

#[gpui::test]
fn t11_the_id_follows_the_name_until_it_is_typed(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    open_from_menu(&page, CUSTOM, cx);
    let dialog = dialog(cx);
    let name = dialog.read_with(cx, |dialog, _| dialog.name.clone());
    assert!(is_focused(&name, cx));

    type_text(cx, "Read the plan");
    assert_eq!(
        form_texts(&dialog, cx),
        ("Read the plan".to_string(), "read-the-plan".to_string())
    );
    // The id was set by the name, not typed: it keeps following.
    assert_eq!(own_id(&dialog, cx), None);

    let id = dialog.read_with(cx, |dialog, _| dialog.id.clone());
    focus(&id, cx);
    press(cx, "end");
    type_text(cx, "-2");
    assert_eq!(own_id(&dialog, cx).as_deref(), Some("read-the-plan-2"));
    focus(&name, cx);
    press(cx, "end");
    type_text(cx, " again");
    assert_eq!(
        form_texts(&dialog, cx),
        (
            "Read the plan again".to_string(),
            "read-the-plan-2".to_string()
        )
    );
}

#[gpui::test]
fn t11_problems_show_after_the_first_try_and_adding_selects_the_stage(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let picking = open_from_menu(&page, CUSTOM, cx);
    let dialog = dialog(cx);
    let steps_before = picking
        .pipeline
        .read_with(cx, |pipeline, _| pipeline.steps().to_vec());
    let shown = |dialog: &Entity<CustomStageDialog>, cx: &mut VisualTestContext| {
        dialog.read_with(cx, |dialog, cx| {
            let input = dialog.input(cx);
            let steps = dialog.pipeline.read(cx).steps();
            visible_problems(&custom_stage_problems(steps, &input), &input, dialog.tried)
        })
    };
    // An empty name waits for the first try.
    assert!(shown(&dialog, cx).is_empty());
    // Mod+Enter is the sheet's Add stage.
    press(cx, "mod-enter");
    let after_try = shown(&dialog, cx);
    assert_eq!(after_try.name.as_deref(), Some("Give the stage a name"));
    let steps_after = picking
        .pipeline
        .read_with(cx, |pipeline, _| pipeline.steps().to_vec());
    assert_eq!(steps_after, steps_before);

    type_text(cx, "Smoke test");
    assert!(shown(&dialog, cx).is_empty());
    press(cx, "mod-enter");
    cx.run_until_parked();
    let (picked, added) = picking.pipeline.read_with(cx, |pipeline, _| {
        (
            pipeline.selected_id(),
            find_stage(pipeline.steps(), "smoke-test").map(|stage| stage.name.clone()),
        )
    });
    assert_eq!(picked.as_deref(), Some("smoke-test"));
    // A name the id gives back ("smoke-test" reads "Smoke test") is left out of the stage.
    assert_eq!(added, Some(None));
    // The custom sheet closed and the new stage's sheet opened (main's T-0313 flow).
    let top = cx.update(|_, cx| DialogLayer::top_view(cx));
    assert!(
        top.and_then(|top| top.downcast::<StageSheet>().ok())
            .is_some()
    );
}
