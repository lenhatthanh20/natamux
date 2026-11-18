//! A pick of Rendered or Source moves the focus to the shown part, so the editor tab's keys
//! still work (T-0323, review rev 1). A CRLF file keeps its line ends (T-0343).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};

use super::FileEditor;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::source::Source;
use crate::app::test_views;

const REVIEWED: &str = "T-0143";

#[path = "editor_tab_golden_tests.rs"]
mod golden;

/// `README.md` of the reviewed task of the fake, read and drawn, with the focus in its editor.
fn open_readme(cx: &mut TestAppContext) -> (Entity<FileEditor>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.update(crate::app::shortcuts::bind);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        FileEditor::new(dashboard, "T-0143".into(), "README.md".into(), window, cx)
    });
    cx.run_until_parked();
    editor.update_in(cx, |editor, window, cx| {
        let body = editor.body.clone().expect("the body is drawn");
        window.focus(&body.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
    (editor, cx)
}

fn mod_key(key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{key}")
}

#[gpui::test]
fn rendered_takes_the_focus_so_find_still_works(cx: &mut TestAppContext) {
    let (editor, cx) = open_readme(cx);
    editor.update_in(cx, |editor, window, cx| {
        editor.set_rendered(true, window, cx)
    });
    cx.run_until_parked();
    let focused = editor.update_in(cx, |editor, window, cx| {
        editor.focus_handle(cx).contains_focused(window, cx)
    });
    assert!(focused, "the focus is in the drawn editor tab");

    // Before the fix the focus stayed in the hidden editor and Mod+F did nothing.
    cx.simulate_keystrokes(&mod_key("f"));
    cx.run_until_parked();
    editor.read_with(cx, |editor, _| {
        assert!(!editor.rendered, "Find goes back to Source");
        assert!(editor.find.is_some(), "the find bar opened");
    });
}

#[gpui::test]
fn source_gives_the_focus_back_to_the_editor(cx: &mut TestAppContext) {
    let (editor, cx) = open_readme(cx);
    editor.update_in(cx, |editor, window, cx| {
        editor.set_rendered(true, window, cx)
    });
    cx.run_until_parked();
    let in_editor = editor.update_in(cx, |editor, window, cx| {
        editor.set_rendered(false, window, cx);
        let body = editor.body.clone().expect("the body is kept");
        body.read(cx).focus_handle(cx).is_focused(window)
    });
    assert!(in_editor);
}

#[gpui::test]
fn a_file_that_is_not_markdown_stays_on_source(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        FileEditor::new(
            dashboard,
            "T-0143".into(),
            "src/checkout.ts".into(),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    editor.update_in(cx, |editor, window, cx| {
        editor.set_rendered(true, window, cx)
    });
    editor.read_with(cx, |editor, _| assert!(!editor.rendered));
}

/// `path` with `text` in the reviewed task of the fake, read and drawn in an editor tab.
fn open_text<'a>(
    cx: &'a mut TestAppContext,
    path: &'static str,
    text: &str,
) -> (Entity<FileEditor>, &'a mut VisualTestContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    fake_daemon::shared().put_workspace_file(REVIEWED, path, text.to_owned());
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        FileEditor::new(dashboard, REVIEWED.into(), path.into(), window, cx)
    });
    cx.run_until_parked();
    (editor, cx)
}

/// The editor's text becomes `text`, as typing would make it.
fn type_text(editor: &Entity<FileEditor>, text: &str, cx: &mut VisualTestContext) {
    let body = editor.read_with(cx, |editor, _| {
        editor.body.clone().expect("the body is drawn")
    });
    body.update(cx, |body, cx| body.view().replace_minimal(text, cx));
    cx.run_until_parked();
}

fn has_draft(editor: &Entity<FileEditor>, cx: &mut VisualTestContext) -> bool {
    editor.read_with(cx, |editor, cx| editor.draft(cx).is_some())
}

fn save(editor: &Entity<FileEditor>, cx: &mut VisualTestContext) {
    editor.update(cx, |editor, cx| editor.save(cx));
    cx.run_until_parked();
}

fn on_disk(path: &str) -> Option<String> {
    let file = fake_daemon::shared().read_workspace_file(REVIEWED, path);
    file.unwrap().text
}

#[gpui::test]
fn a_crlf_file_with_its_own_text_again_is_not_modified(cx: &mut TestAppContext) {
    let (editor, cx) = open_text(cx, "crlf.txt", "a\r\nb\r\n");
    assert!(!has_draft(&editor, cx), "a read makes no draft");
    type_text(&editor, "a\nB\n", cx);
    assert!(has_draft(&editor, cx));
    // Before the fix the LF text was compared with the CRLF text on disk, so it stayed a draft.
    type_text(&editor, "a\nb\n", cx);
    assert!(!has_draft(&editor, cx), "the text on disk again: no draft");
}

#[gpui::test]
fn a_crlf_file_is_saved_with_crlf(cx: &mut TestAppContext) {
    let (editor, cx) = open_text(cx, "crlf.txt", "a\r\nb\r\n");
    type_text(&editor, "a\nB\n", cx);
    save(&editor, cx);
    assert_eq!(on_disk("crlf.txt").as_deref(), Some("a\r\nB\r\n"));
    assert!(
        !has_draft(&editor, cx),
        "the saved text is the text on disk"
    );
}

#[gpui::test]
fn a_file_with_mixed_line_ends_is_saved_with_lf(cx: &mut TestAppContext) {
    let (editor, cx) = open_text(cx, "mixed.txt", "a\r\nb\nc\n");
    type_text(&editor, "a\nB\nc\n", cx);
    save(&editor, cx);
    assert_eq!(on_disk("mixed.txt").as_deref(), Some("a\nB\nc\n"));
}
