//! The find bar and the blame column of the editor tab (advisor A20). The golden files were
//! written on Zed's editor (S4.3a); the own code view (S4.3c) must give the same rows:
//! - find, with Show whitespace on: the highlighted ranges (active match or other match) and the
//!   selection after the term is typed and after each Enter and Shift+Enter, from the start of the
//!   file and from a caret further down;
//! - blame: the rows the editor shows with the labels at their start, and the right-click menu of
//!   every line, off, on and while the file has unsaved edits.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use std::time::Duration;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use theme::ActiveTheme;

use crate::app::dashboard::Dashboard;
use crate::app::golden_files;
use crate::app::options::ThemeMode;
use crate::app::shortcuts::{Back, FindNext, FindPrevious};
use crate::app::source::Source;
use crate::app::test_views;
use crate::app::view_state::ViewState;
use crate::files::file_editor::FileEditor;
use crate::git::code_target::BlameTarget as _;
use crate::git::languages::golden_tests::hex;
use crate::text_view::code_view::CodeView;

const SET: &str = "editor_tab";
const REVIEWED: &str = "T-0143";
const SOURCE: &str = "src/checkout.ts";
/// The term of the `editor-find` shot.
const TERM: &str = "coupon";
/// The caret row of the second find, between the matches, so its first Enter goes to the match
/// after the caret and not to the first one.
const LATER_ROW: u32 = 3;

/// The editor tab of `SOURCE` in the reviewed task of the fake, read and drawn, with Show
/// whitespace on.
fn open(cx: &mut TestAppContext) -> (Entity<FileEditor>, &mut VisualTestContext) {
    let mut view = ViewState::default();
    view.diff.show_whitespace = true;
    cx.update(|cx| test_views::init_with(ThemeMode::Dark, &view, cx));
    cx.update(crate::app::shortcuts::bind);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        FileEditor::new(dashboard, REVIEWED.into(), SOURCE.into(), window, cx)
    });
    cx.run_until_parked();
    (editor, cx)
}

fn text_editor(editor: &Entity<FileEditor>, cx: &mut VisualTestContext) -> CodeView {
    editor.read_with(cx, |editor, cx| {
        let body = editor.body.as_ref().expect("the body is drawn");
        body.read(cx).view().clone()
    })
}

/// Row and byte column of `offset`, as a Zed `Point`.
fn at(text: &str, offset: usize) -> Value {
    let row = text[..offset].matches('\n').count();
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    json!([row, offset - start])
}

/// The highlighted ranges in file order, each as active, match or its colour, and the selection.
fn find_row(step: &str, target: &CodeView, cx: &mut VisualTestContext) -> Value {
    // The other places of the selected text show after a short wait.
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let text = target.text_box().text(cx);
        let other = hex(cx
            .theme()
            .colors()
            .editor_document_highlight_bracket_background);
        let (marks, selection) = target.marks(cx);
        let mut found: Vec<(usize, usize, String)> = marks
            .into_iter()
            .map(|(range, kind)| {
                let kind = match kind {
                    Some(true) => "active".to_owned(),
                    Some(false) => "match".to_owned(),
                    None => other.clone(),
                };
                (range.start, range.end, kind)
            })
            .collect();
        found.sort();
        let found: Vec<Value> = found
            .into_iter()
            .map(|(start, end, kind)| json!([at(&text, start), at(&text, end), kind]))
            .collect();
        json!([
            step,
            found,
            [at(&text, selection.start), at(&text, selection.end)]
        ])
    })
}

/// The term typed into a new bar, then Enter one time more than there are matches, then
/// Shift+Enter twice.
fn find_steps(label: &str, editor: &Entity<FileEditor>, cx: &mut VisualTestContext) -> Vec<Value> {
    let target = text_editor(editor, cx);
    editor.update_in(cx, |editor, window, cx| editor.shot_find(TERM, window, cx));
    cx.run_until_parked();
    let mut rows = vec![find_row(&format!("{label} typed"), &target, cx)];
    let count = rows[0][1].as_array().unwrap().len();
    for press in 1..=count + 1 {
        cx.dispatch_action(FindNext);
        cx.run_until_parked();
        rows.push(find_row(&format!("{label} enter {press}"), &target, cx));
    }
    for press in 1..=2 {
        cx.dispatch_action(FindPrevious);
        cx.run_until_parked();
        rows.push(find_row(
            &format!("{label} shift-enter {press}"),
            &target,
            cx,
        ));
    }
    rows
}

fn find_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (editor, cx) = open(cx);
    let mut rows = find_steps("start", &editor, cx);
    // Esc closes the bar; the next one starts after the caret.
    cx.dispatch_action(Back);
    cx.run_until_parked();
    let target = text_editor(&editor, cx);
    rows.push(find_row("closed", &target, cx));
    editor.update_in(cx, |editor, window, cx| {
        let body = editor.body.clone().expect("the body is drawn");
        body.update(cx, |body, cx| {
            body.go_to(LATER_ROW + 1, Some(1), window, cx)
        });
    });
    cx.run_until_parked();
    rows.extend(find_steps("later", &editor, cx));
    rows
}

/// The blame menu of every line of the file, and of a row of no line of it.
fn menu_rows(label: &str, editor: &Entity<FileEditor>, cx: &mut VisualTestContext) -> Vec<Value> {
    let lines = editor.read_with(cx, |editor, cx| {
        let body = editor.body.as_ref().expect("the body is drawn");
        body.read(cx).view().lines(cx) - 1
    });
    let blame = editor.read_with(cx, |editor, _| editor.blame.clone());
    let rows = (0..=lines).map(Some).chain([None]);
    rows.map(|row| {
        let items = blame.read_with(cx, |blame, cx| blame.menu_for(0, row, cx));
        let items: Vec<String> = items.iter().map(|item| format!("{item:?}")).collect();
        json!([label, row, items])
    })
    .collect()
}

fn blame_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (editor, cx) = open(cx);
    let mut rows = menu_rows("menu off", &editor, cx);
    editor.update(cx, |editor, cx| editor.annotate(cx));
    cx.run_until_parked();
    let target = text_editor(&editor, cx);
    let shown = cx.update(|_, cx| target.shown_text(cx));
    rows.extend(
        shown
            .lines()
            .enumerate()
            .map(|(row, line)| json!(["shown", row, line])),
    );
    rows.extend(menu_rows("menu on", &editor, cx));
    rows
}

fn blocked_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (editor, cx) = open(cx);
    let target = text_editor(&editor, cx);
    cx.update(|window, cx| target.text_box().insert_at_caret("x", window, cx));
    cx.run_until_parked();
    menu_rows("menu with a draft", &editor, cx)
}

#[gpui::test]
fn find_matches_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, "find-checkout", &find_rows(cx));
}

#[gpui::test]
fn blame_matches_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, "blame-checkout", &blame_rows(cx));
}

#[gpui::test]
fn blame_with_a_draft_matches_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, "blame-draft-checkout", &blocked_rows(cx));
}

/// The active matches of a find row.
fn active(row: &Value) -> Vec<Value> {
    let found = row[1].as_array().unwrap();
    found
        .iter()
        .filter(|found| found[2] == "active")
        .cloned()
        .collect()
}

/// Fails when the rows show a find that does not move: a sign that Enter and Shift+Enter did not
/// reach the bar, which a golden file would otherwise keep as the expected result.
fn assert_find_moves(rows: &[Value]) {
    for label in ["start ", "later "] {
        let steps: Vec<&Value> = rows
            .iter()
            .filter(|row| row[0].as_str().is_some_and(|step| step.starts_with(label)))
            .collect();
        let count = steps[0][1].as_array().unwrap().len();
        assert!(count > 1, "{label}: one match or none");
        let enters = &steps[1..=count + 1];
        assert_ne!(
            enters[0][2], steps[0][2],
            "{label}: Enter did not move the selection"
        );
        for enter in enters {
            assert_eq!(active(enter).len(), 1, "{label}: not one active match");
        }
        for pair in enters.windows(2) {
            assert_ne!(
                active(pair[0]),
                active(pair[1]),
                "{label}: Enter did not move the match"
            );
        }
        assert_eq!(active(enters[count]), active(enters[0]), "{label}: no wrap");
        let back = steps[count + 2];
        assert_eq!(
            active(back),
            active(enters[count - 1]),
            "{label}: Shift+Enter"
        );
    }
}

#[gpui::test]
fn the_find_moves_through_the_matches_and_wraps(cx: &mut TestAppContext) {
    assert_find_moves(&find_rows(cx));
}

#[gpui::test]
fn the_blame_labels_show_at_the_line_starts(cx: &mut TestAppContext) {
    let rows = blame_rows(cx);
    let shown = |author: &str| {
        rows.iter()
            .any(|row| row[0] == "shown" && row[2].as_str().is_some_and(|l| l.contains(author)))
    };
    assert!(
        shown("Sam Doe"),
        "the fake's second commit labels its lines"
    );
}

#[gpui::test]
#[ignore]
fn golden_editor_tab_find(cx: &mut TestAppContext) {
    let rows = find_rows(cx);
    assert_find_moves(&rows);
    golden_files::write(SET, "find-checkout", &rows);
}

#[gpui::test]
#[ignore]
fn golden_editor_tab_blame(cx: &mut TestAppContext) {
    golden_files::write(SET, "blame-checkout", &blame_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_editor_tab_blame_draft(cx: &mut TestAppContext) {
    golden_files::write(SET, "blame-draft-checkout", &blocked_rows(cx));
}
