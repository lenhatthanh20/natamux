//! Workflow test T12 (plan 7.5), in the Project settings page of the fake's project: each key in
//! a stage field writes the draft, Use default puts the default back, a switch to another stage
//! closes the skills menu, and the placeholder chips and Insert skill put their text at the
//! cursor. Use default and the chips have no focus stop, so the tests call what their click
//! calls; Insert skill's menu is used with the keyboard.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, point, px};
use natamux_proto::stages::prompt::{PROMPT_PLACEHOLDERS, skill_placeholder};
use natamux_proto::stages::schema::Stage;

use super::insert_at_cursor;
use super::super::editor_fields::{TextField, set_text};
use super::super::editor_find::find_stage;
use super::super::pipeline_editor::PipelineEditor;
use super::super::stage_details::DetailsPane;
use super::super::stage_fields_view::change;
use crate::app::test_driver::{is_focused, press, text_of, type_text};
use crate::app::test_views;
use crate::projects::project_settings_page::ProjectSettingsPage;

/// A catalog stage with its default name and prompt.
const STAGE: &str = "implement";
/// A stage with a name and a prompt of its own.
const OTHER: &str = "reproduce";
/// The skill of the fake's project.
const SKILL: &str = "natamux-stage-check";

struct Fields {
    page: Entity<ProjectSettingsPage>,
    pipeline: Entity<PipelineEditor>,
    details: Entity<DetailsPane>,
}

fn pick(page: Entity<ProjectSettingsPage>, id: &str, cx: &mut VisualTestContext) -> Fields {
    let pipeline = page.read_with(cx, |page, _| page.pipeline().expect("read"));
    let (details, _) = page.read_with(cx, |page, _| page.stage_parts().expect("read"));
    pipeline.update(cx, |pipeline, cx| pipeline.select(id, cx));
    cx.run_until_parked();
    Fields {
        page,
        pipeline,
        details,
    }
}

fn field(fields: &Fields, which: TextField, cx: &mut VisualTestContext) -> Entity<Editor> {
    fields.details.read_with(cx, |details, _| {
        let stage = details.stage_fields.as_ref().expect("a stage is picked");
        match which {
            TextField::Name => stage.name.editor.clone(),
            TextField::Prompt => stage.prompt.editor.clone(),
            _ => unreachable!("only the name and the prompt are used here"),
        }
    })
}

fn stage_of(fields: &Fields, id: &str, cx: &mut VisualTestContext) -> Stage {
    fields.pipeline.read_with(cx, |pipeline, _| {
        find_stage(pipeline.steps(), id).cloned().expect("in the draft")
    })
}

fn focus(view: &Entity<impl Focusable>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(&view.focus_handle(cx), cx));
    cx.run_until_parked();
}

/// What Use default of a field does.
fn use_default(fields: &Fields, which: TextField, cx: &mut VisualTestContext) {
    let pipeline = fields.pipeline.clone();
    cx.update(|_, cx| change(&pipeline, cx, |steps| set_text(steps, STAGE, which, "")));
    cx.run_until_parked();
}

#[gpui::test]
fn t12_each_key_writes_the_draft(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let fields = pick(page, STAGE, cx);
    let name = field(&fields, TextField::Name, cx);
    assert_eq!(text_of(&name, cx), "Implement");
    assert_eq!(stage_of(&fields, STAGE, cx).name, None);
    focus(&name, cx);
    press(cx, "end");
    let mut wrong = Vec::new();
    for key in ["s", "x", "backspace"] {
        if key == "backspace" {
            press(cx, key);
        } else {
            type_text(cx, key);
        }
        let shown = text_of(&name, cx);
        let draft = stage_of(&fields, STAGE, cx).name;
        if draft.as_deref() != Some(shown.as_str()) {
            wrong.push(format!("after {key}: field {shown:?}, draft {draft:?}"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
    assert_eq!(text_of(&name, cx), "Implements");
}

#[gpui::test]
fn t12_use_default_puts_the_default_back(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let fields = pick(page, STAGE, cx);
    let name = field(&fields, TextField::Name, cx);
    focus(&name, cx);
    press(cx, "end");
    type_text(cx, " it");
    assert_eq!(stage_of(&fields, STAGE, cx).name.as_deref(), Some("Implement it"));

    // A click on Use default leaves the focus in the field, which keeps what it shows.
    use_default(&fields, TextField::Name, cx);
    assert_eq!(stage_of(&fields, STAGE, cx).name, None);
    assert_eq!(text_of(&name, cx), "Implement it");

    // Leaving the field changes nothing; the next change of the draft shows the default.
    let page = fields.page.clone();
    focus(&page, cx);
    assert_eq!(text_of(&name, cx), "Implement it");
    fields
        .pipeline
        .update(cx, |pipeline, cx| pipeline.set_picking(false, cx));
    cx.run_until_parked();
    assert_eq!(text_of(&name, cx), "Implement");

    // The prompt: an edit makes it the stage's own; Use default drops it.
    let prompt = field(&fields, TextField::Prompt, cx);
    let default_prompt = text_of(&prompt, cx);
    assert!(!default_prompt.is_empty());
    focus(&prompt, cx);
    press(cx, "mod-a");
    type_text(cx, "Write it.");
    assert_eq!(stage_of(&fields, STAGE, cx).prompt.as_deref(), Some("Write it."));
    focus(&page, cx);
    use_default(&fields, TextField::Prompt, cx);
    assert_eq!(stage_of(&fields, STAGE, cx).prompt, None);
    assert_eq!(text_of(&prompt, cx), default_prompt);
}

#[gpui::test]
fn t12_a_switch_to_another_stage_closes_the_skills_menu(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let fields = pick(page, STAGE, cx);
    fields.details.update_in(cx, |details, window, cx| {
        details.toggle_skills(point(px(10.), px(10.)), window, cx)
    });
    cx.run_until_parked();
    assert!(fields.details.read_with(cx, |details, _| details.skill_menu.is_some()));
    fields
        .pipeline
        .update(cx, |pipeline, cx| pipeline.select(OTHER, cx));
    cx.run_until_parked();
    let (menu, id) = fields.details.read_with(cx, |details, _| {
        let id = details.stage_fields.as_ref().map(|stage| stage.id.clone());
        (details.skill_menu.is_some(), id)
    });
    assert!(!menu);
    assert_eq!(id.as_deref(), Some(OTHER));
}

#[gpui::test]
fn t12_a_chip_goes_in_at_the_cursor(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let fields = pick(page, STAGE, cx);
    let prompt = field(&fields, TextField::Prompt, cx);
    let before = text_of(&prompt, cx);
    focus(&prompt, cx);
    press(cx, "mod-a left");
    let page = fields.page.clone();
    focus(&page, cx);
    // The click of the first chip; it gives the focus to the prompt.
    let token = format!("{{{}}}", PROMPT_PLACEHOLDERS[0]);
    cx.update(|window, cx| insert_at_cursor(&prompt, &token, window, cx));
    cx.run_until_parked();
    assert!(is_focused(&prompt, cx));
    let after = text_of(&prompt, cx);
    assert_eq!(after, format!("{token}{before}"));
    assert_eq!(stage_of(&fields, STAGE, cx).prompt, Some(after));
}

#[gpui::test]
fn t12_insert_skill_puts_the_skill_at_the_cursor(cx: &mut TestAppContext) {
    let (page, _, cx) = test_views::init_project_page(cx);
    let fields = pick(page, STAGE, cx);
    let prompt = field(&fields, TextField::Prompt, cx);
    let before = text_of(&prompt, cx);
    focus(&prompt, cx);
    press(cx, "mod-a left");
    fields.details.update_in(cx, |details, window, cx| {
        details.toggle_skills(point(px(10.), px(10.)), window, cx)
    });
    cx.run_until_parked();
    press(cx, "down enter");
    let after = text_of(&prompt, cx);
    assert_eq!(after, format!("{}{before}", skill_placeholder(SKILL)));
    assert_eq!(stage_of(&fields, STAGE, cx).prompt, Some(after));
    assert!(is_focused(&prompt, cx));
    assert!(fields.details.read_with(cx, |details, _| details.skill_menu.is_none()));
}
