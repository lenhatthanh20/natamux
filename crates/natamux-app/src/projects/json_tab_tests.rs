//! Workflow test T14 (plan 7.5): the JSON tab writes good and bad JSON into the draft, a text set
//! from outside keeps the caret and the undo history, keys typed while that text goes in are
//! kept, and a language that loads after typing does not change the text.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use gpui::{AnyView, AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::JsonTab;
use crate::app::test_driver::{paste, press, press_now, selection_of, text_of, type_text};
use crate::app::test_views;
use crate::stages::pipeline_editor::PipelineEditor;

const DRAFT: &str = "{\n  \"agents\": {\n    \"maxSendBacks\": 2\n  }\n}\n";
/// `DRAFT` as another part of the page sets it: one value changed, after the caret at the start.
const CHANGED: &str = "{\n  \"agents\": {\n    \"maxSendBacks\": 3\n  }\n}\n";

fn start(
    cx: &mut TestAppContext,
) -> (
    Entity<PipelineEditor>,
    Entity<Editor>,
    &mut VisualTestContext,
) {
    let (tab, _, cx) = test_views::init_app(cx, |_, window, cx| {
        let pipeline = cx.new(|_| PipelineEditor::new(DRAFT));
        cx.new(|cx| JsonTab::new(pipeline, window, cx))
    });
    let (pipeline, editor) = tab.read_with(cx, |tab, _| (tab.pipeline.clone(), tab.editor.clone()));
    cx.update(|window, cx| window.focus(&editor.focus_handle(cx), cx));
    cx.run_until_parked();
    // The caret at the start.
    press(cx, "mod-a left");
    (pipeline, editor, cx)
}

fn draft(pipeline: &Entity<PipelineEditor>, cx: &mut VisualTestContext) -> (String, bool) {
    pipeline.read_with(cx, |pipeline, _| {
        (pipeline.text().to_owned(), pipeline.parse_error().is_some())
    })
}

#[gpui::test]
fn t14_good_and_bad_json_go_into_the_draft(cx: &mut TestAppContext) {
    let (pipeline, editor, cx) = start(cx);
    assert_eq!(draft(&pipeline, cx), (DRAFT.to_string(), false));
    type_text(cx, "x");
    assert_eq!(draft(&pipeline, cx), (format!("x{DRAFT}"), true));
    press(cx, "backspace");
    assert_eq!(draft(&pipeline, cx), (DRAFT.to_string(), false));
    press(cx, "mod-a backspace");
    assert_eq!(draft(&pipeline, cx), (String::new(), true));
    paste(cx, CHANGED);
    assert_eq!(draft(&pipeline, cx), (CHANGED.to_string(), false));
    assert_eq!(text_of(&editor, cx), CHANGED);
    let most = pipeline.read_with(cx, |pipeline, _| pipeline.max_send_backs());
    assert_eq!(most, Some(3));
}

#[gpui::test]
fn t14_a_text_from_outside_keeps_the_caret_and_the_undo(cx: &mut TestAppContext) {
    let (pipeline, editor, cx) = start(cx);
    type_text(cx, " ");
    assert_eq!(selection_of(&editor, cx), 1..1);
    // The buffer joins edits less than 300 ms apart (real time) into one undo step. An outside
    // change comes well after a typed key in the app, so wait longer than that here too.
    std::thread::sleep(std::time::Duration::from_millis(400));
    let outside = format!(" {CHANGED}");
    pipeline.update(cx, |pipeline, cx| pipeline.set_text(&outside, cx));
    cx.run_until_parked();
    assert_eq!(text_of(&editor, cx), outside);
    assert_eq!(selection_of(&editor, cx), 1..1);
    assert_eq!(draft(&pipeline, cx), (outside.clone(), false));

    // The first undo takes back the outside text, the second the typed space.
    press(cx, "mod-z");
    let once = text_of(&editor, cx);
    let once_draft = draft(&pipeline, cx).0;
    press(cx, "mod-z");
    let twice = text_of(&editor, cx);
    let before = format!(" {DRAFT}");
    assert_eq!(
        (once.as_str(), once_draft.as_str(), twice.as_str()),
        (before.as_str(), before.as_str(), DRAFT),
        "undo after a text from outside"
    );
    assert_eq!(draft(&pipeline, cx), (DRAFT.to_string(), false));
}

#[gpui::test]
fn t14_keys_typed_while_an_outside_text_goes_in_are_kept(cx: &mut TestAppContext) {
    let (pipeline, editor, cx) = start(cx);
    pipeline.update(cx, |pipeline, cx| pipeline.set_text(CHANGED, cx));
    // The outside text goes in after a diff off the main thread; these keys come first.
    press_now(cx, "z");
    cx.run_until_parked();
    let shown = text_of(&editor, cx);
    assert_eq!(shown, format!("z{CHANGED}"));
    assert_eq!(draft(&pipeline, cx).0, shown);
}

/// Holds the tab once it is made, so the test decides when its language load may run.
struct Slot {
    view: Option<AnyView>,
}

impl Render for Slot {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().children(self.view.clone())
    }
}

#[gpui::test]
fn t14_a_language_that_loads_after_typing_keeps_the_text(cx: &mut TestAppContext) {
    let (slot, _, cx) = test_views::init_app(cx, |_, _, cx| cx.new(|_| Slot { view: None }));
    cx.update(|_, cx| crate::git::languages::init(cx));
    let (pipeline, editor) = slot.update_in(cx, |slot, window, cx| {
        let pipeline = cx.new(|_| PipelineEditor::new(DRAFT));
        let tab = cx.new(|cx| JsonTab::new(pipeline.clone(), window, cx));
        let editor = tab.read(cx).editor.clone();
        slot.view = Some(tab.into());
        cx.notify();
        window.focus(&editor.focus_handle(cx), cx);
        (pipeline, editor)
    });
    // Typed before the JSON language is loaded: no task has run yet.
    press_now(cx, "space space");
    cx.run_until_parked();
    let language = editor.read_with(cx, |editor, cx| {
        let buffer = editor.buffer().read(cx).as_singleton().expect("one buffer");
        buffer
            .read(cx)
            .language()
            .map(|language| language.name().to_string())
    });
    assert_eq!(language.as_deref(), Some("JSON"));
    let shown = text_of(&editor, cx);
    assert_eq!(shown, format!("  {DRAFT}"));
    assert_eq!(draft(&pipeline, cx).0, shown);
}
