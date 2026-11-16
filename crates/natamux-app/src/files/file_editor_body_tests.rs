//! T-0316: undo never edits a newer disk text back; clean on save; go to line.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, Focusable as _, TestAppContext, VisualTestContext};

use super::{FileEditorBody, cleaned, line_offset};
use crate::app::test_views;
use crate::git::code_target::FindTarget as _;
use crate::text_view::text_input::actions::Undo;

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
    body.update_in(cx, |body, window, cx| {
        body.view().text_box().insert_at_caret(typed, window, cx)
    });
    cx.run_until_parked();
}

fn undo(body: &Entity<FileEditorBody>, cx: &mut VisualTestContext) {
    body.update_in(cx, |body, window, cx| {
        window.focus(&body.focus_handle(cx), cx)
    });
    cx.dispatch_action(Undo);
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

/// The caret, as a byte offset.
fn cursor(body: &Entity<FileEditorBody>, cx: &mut VisualTestContext) -> usize {
    body.update(cx, |body, cx| body.view().caret(cx))
}

/// Review 2: the column counts characters, not bytes; a byte column inside `ế` panicked.
#[gpui::test]
fn go_to_counts_the_column_in_characters(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "tiếng\n");
    body.update_in(cx, |body, window, cx| body.go_to(1, Some(3), window, cx));
    assert_eq!(cursor(&body, cx), 2);
    body.update_in(cx, |body, window, cx| body.go_to(1, Some(4), window, cx));
    assert_eq!(cursor(&body, cx), 5);
    body.update_in(cx, |body, window, cx| body.go_to(1, Some(99), window, cx));
    assert_eq!(cursor(&body, cx), 7);
}

#[gpui::test]
fn go_to_clamps_the_line_and_the_column(cx: &mut TestAppContext) {
    let (body, cx) = open(cx, "ab\ncdef\ng\n");
    body.update_in(cx, |body, window, cx| body.go_to(2, Some(3), window, cx));
    assert_eq!(cursor(&body, cx), 5);
    body.update_in(cx, |body, window, cx| body.go_to(2, Some(99), window, cx));
    assert_eq!(cursor(&body, cx), 7);
    body.update_in(cx, |body, window, cx| body.go_to(99, None, window, cx));
    assert_eq!(cursor(&body, cx), 10);
}

#[test]
fn cleaned_turns_trailing_whitespace_at_the_end_into_one_line_break() {
    assert_eq!(cleaned("a \n\t\n \n"), "a\n");
    assert_eq!(cleaned("  \n"), "\n");
    assert_eq!(cleaned("a\r\n"), "a\n");
}

#[test]
fn line_offset_clamps_past_the_last_line() {
    assert_eq!(line_offset("ab\ncd", 9, Some(9)), 5);
    assert_eq!(line_offset("ab\ncd", 0, None), 0);
}
