//! T-0316: Comment on a change quotes its lines in the comment box; one change of an inline diff
//! closes and opens again; ignore whitespace leaves no hunk for a whitespace-only change.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use gpui::{Entity, TestAppContext, VisualTestContext};
use ui::prelude::*;

use crate::app::shortcuts::CommentOnDiff;
use crate::app::test_views;
use crate::git::change_rules::Side;
use crate::git::diff_view::{DiffOptions, DiffView};

fn numbered(word: &str, count: u32) -> String {
    (0..count).map(|line| format!("{word} {line}\n")).collect()
}

/// Rows 10–12 changed, rows 30–31 of the old text deleted.
fn texts() -> (String, String) {
    let original =
        numbered("a", 10) + &numbered("old", 3) + &numbered("b", 17) + &numbered("gone", 2);
    let modified = numbered("a", 10) + &numbered("new", 3) + &numbered("b", 17);
    (original, modified)
}

/// The Changes tab around the diff, as far as Comment goes.
struct Host {
    diff: Entity<DiffView>,
    comments: usize,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("NatamuxChanges")
            .size_full()
            .on_action(cx.listener(|host, _: &CommentOnDiff, _, _| host.comments += 1))
            .child(self.diff.clone())
    }
}

fn open(
    cx: &mut TestAppContext,
    side_by_side: bool,
    ignore_whitespace: bool,
    (original, modified): (String, String),
) -> (Entity<Host>, Entity<DiffView>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.update(crate::app::shortcuts::bind);
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        diff: cx.new(|_| DiffView::new()),
        comments: 0,
    });
    let view = host.read_with(cx, |host, _| host.diff.clone());
    let options = DiffOptions {
        side_by_side,
        collapse: false,
        wrap: false,
        ignore_whitespace,
    };
    view.update_in(cx, |view, window, cx| {
        view.show(
            "file",
            "notes.txt",
            1,
            Some(&original),
            &modified,
            options,
            window,
            cx,
        )
    });
    cx.run_until_parked();
    (host, view, cx)
}

#[gpui::test]
fn comment_quotes_the_lines_of_the_change(cx: &mut TestAppContext) {
    let (host, view, cx) = open(cx, true, false, texts());
    view.update_in(cx, |view, window, cx| view.comment_hunk(0, window, cx));
    cx.run_until_parked();
    assert_eq!(host.read_with(cx, |host, _| host.comments), 1);
    let quote = view.update_in(cx, |view, window, cx| view.selection(window, cx));
    let quote = quote.expect("the change is picked");
    assert_eq!((quote.side, quote.start, quote.end), (Side::New, 11, 13));
}

/// A deletion has no working-tree lines: the comment goes on the line after it, here the last.
#[gpui::test]
fn comment_on_a_deletion_quotes_the_line_after_it(cx: &mut TestAppContext) {
    let (_, view, cx) = open(cx, false, false, texts());
    let count = view.read_with(cx, |view, _| view.hunks.len());
    assert_eq!(count, 2);
    view.update_in(cx, |view, window, cx| view.comment_hunk(1, window, cx));
    cx.run_until_parked();
    let quote = view.update_in(cx, |view, window, cx| view.selection(window, cx));
    let quote = quote.expect("a line is picked");
    assert_eq!(quote.side, Side::New);
    assert!(quote.start >= 30 && quote.end >= quote.start, "{quote:?}");
}

fn expanded(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> bool {
    editor.update(cx, |editor, cx| {
        let range = editor::Anchor::Min..editor::Anchor::Max;
        editor
            .buffer()
            .read(cx)
            .has_expanded_diff_hunks_in_ranges(&[range], cx)
    })
}

#[gpui::test]
fn one_change_of_an_inline_diff_closes_and_opens_again(cx: &mut TestAppContext) {
    let (_, view, cx) = open(cx, false, false, (numbered("x", 3), numbered("y", 3)));
    let right = view.read_with(cx, |view, _| {
        view.sides.as_ref().expect("drawn").right.clone()
    });
    assert!(expanded(&right, cx));
    view.update_in(cx, |view, window, cx| {
        view.go_to_line(Side::New, 1, window, cx);
        view.focus(window, cx);
    });
    cx.run_until_parked();
    let toggle = if cfg!(target_os = "macos") {
        "cmd-'"
    } else {
        "ctrl-'"
    };
    cx.simulate_keystrokes(toggle);
    cx.run_until_parked();
    assert!(!expanded(&right, cx));
    cx.simulate_keystrokes(toggle);
    cx.run_until_parked();
    assert!(expanded(&right, cx));
}

#[gpui::test]
fn side_by_side_keeps_every_change_open(cx: &mut TestAppContext) {
    let (_, view, cx) = open(cx, true, false, (numbered("x", 3), numbered("y", 3)));
    let right = view.read_with(cx, |view, _| {
        view.sides.as_ref().expect("drawn").right.clone()
    });
    view.update_in(cx, |view, window, cx| view.focus(window, cx));
    cx.run_until_parked();
    let toggle = if cfg!(target_os = "macos") {
        "cmd-'"
    } else {
        "ctrl-'"
    };
    cx.simulate_keystrokes(toggle);
    cx.run_until_parked();
    assert!(expanded(&right, cx));
}

fn indent_texts() -> (String, String) {
    let original = "fn a() {\n  b();\n  c();\n}\n".to_owned();
    let modified = "fn a() {\n    b();  \n    d();\n}\n".to_owned();
    (original, modified)
}

#[gpui::test]
fn ignoring_whitespace_leaves_only_the_real_change(cx: &mut TestAppContext) {
    let (_, view, cx) = open(cx, true, true, indent_texts());
    let hunks = view.read_with(cx, |view, _| view.hunks.clone());
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    assert_eq!((hunks[0].old.clone(), hunks[0].new.clone()), (2..3, 2..3));
}

#[gpui::test]
fn whitespace_counts_while_the_toggle_is_off(cx: &mut TestAppContext) {
    let (_, view, cx) = open(cx, true, false, indent_texts());
    let hunks = view.read_with(cx, |view, _| view.hunks.clone());
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    assert_eq!((hunks[0].old.clone(), hunks[0].new.clone()), (1..3, 1..3));
}
