//! T-0316: undo never edits a newer disk text back; clean on save; go to line.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::actions::Undo;
use gpui::{Entity, TestAppContext, VisualTestContext};
use text::Point;

use super::FileEditorBody;
use crate::app::test_views;

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<FileEditorBody>, &'a mut VisualTestContext) {
    test_views::init(cx);
    let text = text.to_owned();
    cx.add_window_view(move |window, cx| FileEditorBody::new("notes.txt", &text, window, cx))
}

fn text(body: &Entity<FileEditorBody>, cx: &mut VisualTestContext) -> String {
    body.read_with(cx, |body, cx| body.text(cx))
}

fn type_text(body: &Entity<FileEditorBody>, typed: &str, cx: &mut VisualTestContext) {
    let editor = body.read_with(cx, |body, _| body.editor().clone());
    editor.update_in(cx, |editor, window, cx| editor.insert(typed, window, cx));
    cx.run_until_parked();
}

fn undo(body: &Entity<FileEditorBody>, cx: &mut VisualTestContext) {
    let editor = body.read_with(cx, |body, _| body.editor().clone());
    editor.update_in(cx, |editor, window, cx| editor.undo(&Undo, window, cx));
    cx.run_until_parked();
}

fn set_disk(body: &Entity<FileEditorBody>, disk: &str, cx: &mut VisualTestContext) {
    body.update(cx, |body, cx| body.set_disk_text(disk, cx));
    cx.run_until_parked();
}

/// T-0316 part 9: Undo after a reload brought the old text back over the disk version.
#[gpui::test]
fn undo_after_a_disk_reload_keeps_the_disk_text(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "one\n");
    type_text(&body, "typed ", cx);
    set_disk(&body, "disk two\n", cx);
    assert_eq!(text(&body, cx), "disk two\n");
    undo(&body, cx);
    undo(&body, cx);
    assert_eq!(text(&body, cx), "disk two\n");
}

#[gpui::test]
fn undo_after_a_reload_takes_back_only_what_was_typed_since(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "one\n");
    set_disk(&body, "two\n", cx);
    set_disk(&body, "three\n", cx);
    type_text(&body, "x", cx);
    undo(&body, cx);
    assert_eq!(text(&body, cx), "three\n");
}

fn clean(body: &Entity<FileEditorBody>, cx: &mut VisualTestContext) {
    let task = body.update(cx, |body, cx| body.clean(cx));
    cx.run_until_parked();
    drop(task);
}

#[gpui::test]
fn clean_trims_line_ends_and_ends_with_one_line_break(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "a  \nb\t\nc");
    clean(&body, cx);
    assert_eq!(text(&body, cx), "a\nb\nc\n");
    undo(&body, cx);
    assert_eq!(text(&body, cx), "a  \nb\t\nc");
}

#[gpui::test]
fn clean_leaves_a_clean_text_as_it_is(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "a\nb\n");
    clean(&body, cx);
    assert_eq!(text(&body, cx), "a\nb\n");
}

#[gpui::test]
fn clean_folds_extra_final_line_breaks_into_one(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "a\n\n\n");
    clean(&body, cx);
    assert_eq!(text(&body, cx), "a\n");
}

#[gpui::test]
fn clean_keeps_an_empty_file_empty(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "");
    clean(&body, cx);
    assert_eq!(text(&body, cx), "");
}

fn cursor(body: &Entity<FileEditorBody>, cx: &mut VisualTestContext) -> Point {
    let editor = body.read_with(cx, |body, _| body.editor().clone());
    editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        editor.selections.newest::<Point>(&snapshot).head()
    })
}

/// Review 2: the column counts characters, not bytes; a byte column inside `ế` panicked.
#[gpui::test]
fn go_to_counts_the_column_in_characters(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "tiếng\n");
    body.update_in(cx, |body, window, cx| body.go_to(1, Some(3), window, cx));
    assert_eq!(cursor(&body, cx), Point::new(0, 2));
    body.update_in(cx, |body, window, cx| body.go_to(1, Some(4), window, cx));
    assert_eq!(cursor(&body, cx), Point::new(0, 5));
    body.update_in(cx, |body, window, cx| body.go_to(1, Some(99), window, cx));
    assert_eq!(cursor(&body, cx), Point::new(0, 7));
}

#[gpui::test]
fn go_to_clamps_the_line_and_the_column(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "ab\ncdef\ng\n");
    body.update_in(cx, |body, window, cx| body.go_to(2, Some(3), window, cx));
    assert_eq!(cursor(&body, cx), Point::new(1, 2));
    body.update_in(cx, |body, window, cx| body.go_to(2, Some(99), window, cx));
    assert_eq!(cursor(&body, cx), Point::new(1, 4));
    body.update_in(cx, |body, window, cx| body.go_to(99, None, window, cx));
    assert_eq!(cursor(&body, cx), Point::new(3, 0));
}
