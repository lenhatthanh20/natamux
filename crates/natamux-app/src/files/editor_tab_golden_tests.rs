//! The find bar and the blame column of the editor tab on Zed's editor (advisor A20, S4.3a), so
//! the code view of S4.3c must give the same rows:
//! - find, with Show whitespace on: the highlighted ranges (active match or other match) and the
//!   selection after the term is typed and after each Enter and Shift+Enter, from the start of the
//!   file and from a caret further down;
//! - blame: the rows the editor shows with the labels at their start, and the right-click menu of
//!   every line, off, on and while the file has unsaved edits.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use editor::{Editor, MultiBufferOffset};
use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use text::Point;
use theme::ActiveTheme;

use crate::files::file_editor::FileEditor;
use crate::app::dashboard::Dashboard;
use crate::app::golden_files;
use crate::app::options::ThemeMode;
use crate::app::shortcuts::{Back, FindNext, FindPrevious};
use crate::app::source::Source;
use crate::app::test_views;
use crate::app::view_state::ViewState;
use crate::git::languages::golden_tests::hex;

const SET: &str = "editor_tab";
const REVIEWED: &str = "T-0143";
const SOURCE: &str = "src/checkout.ts";
/// The term of the `editor-find` shot.
const TERM: &str = "coupon";
/// The caret row of the second find, below the first matches.
const LATER_ROW: u32 = 12;

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

fn text_editor(editor: &Entity<FileEditor>, cx: &mut VisualTestContext) -> Entity<Editor> {
    editor.read_with(cx, |editor, cx| {
        let body = editor.body.as_ref().expect("the body is drawn");
        body.read(cx).editor().clone()
    })
}

fn at(point: Point) -> Value {
    json!([point.row, point.column])
}

/// The highlighted ranges in file order, each as active, match or its colour, and the selection.
fn find_row(step: &str, target: &Entity<Editor>, cx: &mut VisualTestContext) -> Value {
    target.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        let buffer = snapshot.buffer_snapshot();
        let all = buffer.anchor_before(MultiBufferOffset(0))..buffer.anchor_after(buffer.len());
        let theme = cx.theme().clone();
        let colors = theme.colors();
        let mut found: Vec<(Point, Point, String)> = editor
            .background_highlights_in_range(all, &snapshot, &theme)
            .into_iter()
            .map(|(range, color)| {
                let kind = if color == colors.search_active_match_background {
                    "active".to_owned()
                } else if color == colors.search_match_background {
                    "match".to_owned()
                } else {
                    hex(color)
                };
                (
                    range.start.to_point(&snapshot),
                    range.end.to_point(&snapshot),
                    kind,
                )
            })
            .collect();
        found.sort();
        let found: Vec<Value> = found
            .into_iter()
            .map(|(start, end, kind)| json!([at(start), at(end), kind]))
            .collect();
        let selection = editor.selections.newest::<Point>(&snapshot);
        json!([step, found, [at(selection.start), at(selection.end)]])
    })
}

/// The term typed into a new bar, then Enter one time more than there are matches, then
/// Shift+Enter twice.
fn find_steps(
    label: &str,
    editor: &Entity<FileEditor>,
    cx: &mut VisualTestContext,
) -> Vec<Value> {
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
        rows.push(find_row(&format!("{label} shift-enter {press}"), &target, cx));
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
        body.update(cx, |body, cx| body.go_to(LATER_ROW + 1, Some(1), window, cx));
    });
    cx.run_until_parked();
    rows.extend(find_steps("later", &editor, cx));
    rows
}

/// The blame menu of every line of the file, and of a row of no line of it.
fn menu_rows(
    label: &str,
    editor: &Entity<FileEditor>,
    cx: &mut VisualTestContext,
) -> Vec<Value> {
    let lines = editor.read_with(cx, |editor, cx| {
        let body = editor.body.as_ref().expect("the body is drawn");
        body.read(cx).buffer().read(cx).max_point().row
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
    let shown = target.update(cx, |editor, cx| editor.display_text(cx));
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
    target.update_in(cx, |editor, window, cx| editor.insert("x", window, cx));
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

#[gpui::test]
fn the_find_term_has_more_than_one_match(cx: &mut TestAppContext) {
    let rows = find_rows(cx);
    assert!(rows[0][1].as_array().is_some_and(|found| found.len() > 1));
}

#[gpui::test]
fn the_blame_labels_show_at_the_line_starts(cx: &mut TestAppContext) {
    let rows = blame_rows(cx);
    let shown = |author: &str| {
        rows.iter()
            .any(|row| row[0] == "shown" && row[2].as_str().is_some_and(|l| l.contains(author)))
    };
    assert!(shown("Sam Doe"), "the fake's second commit labels its lines");
}

#[gpui::test]
#[ignore]
fn golden_editor_tab_find(cx: &mut TestAppContext) {
    golden_files::write(SET, "find-checkout", &find_rows(cx));
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
