//! Workflow test T13 (plan 7.5), the idle pause part: 0 or 5 to 1440 write the draft at once,
//! other values show the error and leave the draft as the last good key made it, and a change of
//! the draft from elsewhere shows in the field only while it does not have the focus.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};

use super::IdlePauseField;
use super::super::settings_tab_rules::IDLE_PAUSE_ERROR;
use crate::app::test_driver::{press, text_of, type_text};
use crate::app::test_views;
use crate::stages::pipeline_editor::PipelineEditor;

const DRAFT: &str = r#"{ "agents": { "pauseIdleAfterMinutes": 60 } }"#;

fn start(cx: &mut TestAppContext) -> (Entity<IdlePauseField>, &mut VisualTestContext) {
    let (field, _, cx) = test_views::init_app(cx, |_, window, cx| {
        let pipeline = cx.new(|_| PipelineEditor::new(DRAFT));
        cx.new(|cx| IdlePauseField::new(pipeline, window, cx))
    });
    let editor = field.read_with(cx, |field, _| field.editor.clone());
    cx.update(|window, cx| window.focus(&editor.focus_handle(cx), cx));
    cx.run_until_parked();
    (field, cx)
}

fn state(field: &Entity<IdlePauseField>, cx: &mut VisualTestContext) -> (String, Option<u64>, Option<&'static str>) {
    let (editor, minutes, error) = field.read_with(cx, |field, cx| {
        let minutes = field.pipeline.read(cx).pause_idle_minutes();
        (field.editor.clone(), minutes, field.error)
    });
    (text_of(&editor, cx), minutes, error)
}

#[gpui::test]
fn t13_idle_pause_good_values_write_the_draft_and_bad_ones_do_not(cx: &mut TestAppContext) {
    let (field, cx) = start(cx);
    assert_eq!(state(&field, cx), ("60".to_string(), Some(60), None));
    let bad = Some(IDLE_PAUSE_ERROR);
    // Typed key by key into the emptied box: "1440" passes 14 and 144 on the way.
    let cases: [(&str, Option<u64>, Option<&str>); 7] = [
        ("0", Some(0), None),
        ("5", Some(5), None),
        ("1440", Some(1440), None),
        ("1441", Some(144), bad),
        ("4", None, bad),
        ("", None, None),
        ("x", None, bad),
    ];
    let mut wrong = Vec::new();
    for (typed, minutes, error) in cases {
        // Mod+A and Backspace leave the box empty, which is a good value too.
        press(cx, "mod-a backspace");
        if !typed.is_empty() {
            type_text(cx, typed);
        }
        let got = state(&field, cx);
        if got != (typed.to_string(), minutes, error) {
            wrong.push(format!("{typed:?}: got {got:?}, want {minutes:?} {error:?}"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t13_idle_pause_shows_a_change_from_elsewhere_only_without_the_focus(cx: &mut TestAppContext) {
    let (field, cx) = start(cx);
    let pipeline = field.read_with(cx, |field, _| field.pipeline.clone());
    press(cx, "mod-a");
    type_text(cx, "2");
    pipeline.update(cx, |pipeline, cx| pipeline.set_pause_idle_minutes(Some(90), cx));
    cx.run_until_parked();
    assert_eq!(state(&field, cx), ("2".to_string(), Some(90), Some(IDLE_PAUSE_ERROR)));

    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert_eq!(state(&field, cx).0, "2");
    pipeline.update(cx, |pipeline, cx| pipeline.set_pause_idle_minutes(Some(15), cx));
    cx.run_until_parked();
    assert_eq!(state(&field, cx), ("15".to_string(), Some(15), None));
}
