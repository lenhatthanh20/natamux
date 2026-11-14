//! A pick of Rendered or Source moves the focus to the shown part, so the editor tab's keys
//! still work (T-0323, review rev 1).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};

use super::FileEditor;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;

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
        window.focus(&body.read(cx).editor().focus_handle(cx), cx);
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
        body.read(cx).editor().focus_handle(cx).is_focused(window)
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
