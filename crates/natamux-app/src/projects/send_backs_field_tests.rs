//! Workflow test T13 (plan 7.5), the send backs part: a good value writes the draft at once, a
//! bad one shows its error and leaves the draft as the last good key made it, and a change of
//! the draft from elsewhere shows in the field only while it does not have the focus.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};

use super::super::settings_tab_rules::SEND_BACKS_ERROR;
use super::SendBacksField;
use crate::app::test_driver::{press, text_of, type_text};
use crate::app::test_views;
use crate::stages::pipeline_editor::PipelineEditor;

const DRAFT: &str = r#"{ "agents": { "maxSendBacks": 2 } }"#;

fn start(cx: &mut TestAppContext) -> (Entity<SendBacksField>, &mut VisualTestContext) {
    let (field, _, cx) = test_views::init_app(cx, |_, window, cx| {
        let pipeline = cx.new(|_| PipelineEditor::new(DRAFT));
        cx.new(|cx| SendBacksField::new(pipeline, window, cx))
    });
    let editor = field.read_with(cx, |field, _| field.editor.clone());
    cx.update(|window, cx| window.focus(&editor.focus_handle(cx), cx));
    cx.run_until_parked();
    (field, cx)
}

/// The text, the draft's value and the error after the field shows `typed`.
fn state(
    field: &Entity<SendBacksField>,
    cx: &mut VisualTestContext,
) -> (String, Option<usize>, Option<&'static str>) {
    let (editor, most, error) = field.read_with(cx, |field, cx| {
        let most = field.pipeline.read(cx).max_send_backs();
        (field.editor.clone(), most, field.error)
    });
    (text_of(&editor, cx), most, error)
}

#[gpui::test]
fn t13_send_backs_good_values_write_the_draft_and_bad_ones_do_not(cx: &mut TestAppContext) {
    let (field, cx) = start(cx);
    assert_eq!(state(&field, cx), ("2".to_string(), Some(2), None));
    let bad = Some(SEND_BACKS_ERROR);
    // Typed key by key into the emptied box: each key writes the draft when the text is good.
    let cases: [(&str, Option<usize>, Option<&str>); 6] = [
        ("5", Some(5), None),
        ("0", Some(0), None),
        ("", None, None),
        ("3x", Some(3), bad),
        (" 4 ", Some(4), None),
        ("-1", None, bad),
    ];
    let mut wrong = Vec::new();
    for (typed, most, error) in cases {
        // Mod+A and Backspace leave the box empty, which is a good value too.
        press(cx, "mod-a backspace");
        if !typed.is_empty() {
            type_text(cx, typed);
        }
        let got = state(&field, cx);
        if got != (typed.to_string(), most, error) {
            wrong.push(format!("{typed:?}: got {got:?}, want {most:?} {error:?}"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t13_send_backs_show_a_change_from_elsewhere_only_without_the_focus(cx: &mut TestAppContext) {
    let (field, cx) = start(cx);
    let pipeline = field.read_with(cx, |field, _| field.pipeline.clone());
    press(cx, "mod-a");
    type_text(cx, "x");
    pipeline.update(cx, |pipeline, cx| pipeline.set_max_send_backs(Some(9), cx));
    cx.run_until_parked();
    assert_eq!(
        state(&field, cx),
        ("x".to_string(), Some(9), Some(SEND_BACKS_ERROR))
    );

    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    // Leaving the field changes nothing; the next change of the draft shows.
    assert_eq!(state(&field, cx).0, "x");
    pipeline.update(cx, |pipeline, cx| pipeline.set_max_send_backs(Some(8), cx));
    cx.run_until_parked();
    assert_eq!(state(&field, cx), ("8".to_string(), Some(8), None));
    pipeline.update(cx, |pipeline, cx| pipeline.set_max_send_backs(None, cx));
    cx.run_until_parked();
    assert_eq!(state(&field, cx), (String::new(), None, None));
}
