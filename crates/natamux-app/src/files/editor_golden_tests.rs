//! T-OUT-1 (advisor A20): what the editor tab draws from the file's syntax past its colours, on
//! Zed's editor, so the own code view (S4.3c) must give the same. Per file:
//! - `item`: each outline item of Go to symbol, with its depth, text and ranges;
//! - `sticky`: for each top row, the items sticky scroll asks for (those in the 10 rows from it);
//! - `guide`: each indent guide, with its rows, depth and tab size;
//! - `active`: for each caret row, the guides drawn as active.
//!
//! Which sticky rows show at a scroll position is not readable from a test (the element works it
//! out while it paints), so the shots `workspace-editor-scrollbar` and `editor-sticky` hold that.
//! The files are the ones of T-HL-1, the long file of the editor shots and the sticky shot's file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use text::{Point, ToPoint};

use super::ZedBody;
use crate::app::golden_files;
use crate::app::shots_text::{LONG_FILE, STICKY_FILE, long_file, sticky_file};
use crate::app::test_views;
use crate::git::languages::golden_tests::fixtures;

const SET: &str = "outline";
/// The rows sticky scroll looks at below the top row.
const STICKY_ROWS: u32 = 10;

/// The T-HL-1 files, then the long file and the sticky file of the editor shots.
fn files() -> Vec<(String, String, String)> {
    let mut out = fixtures();
    out.push(("file-long".to_owned(), LONG_FILE.to_owned(), long_file()));
    out.push((
        "file-sticky".to_owned(),
        STICKY_FILE.to_owned(),
        sticky_file(),
    ));
    out
}

fn at(point: Point) -> Value {
    json!([point.row, point.column])
}

/// The head, the outline items and the sticky items of the file.
fn outline_rows(path: &str, body: &Entity<ZedBody>, cx: &mut VisualTestContext) -> Vec<Value> {
    cx.update(|_, cx| {
        let body = body.read(cx);
        let buffer = body.buffer().read(cx);
        let snapshot = buffer.snapshot();
        let language = buffer
            .language()
            .map(|language| language.name().to_string());
        let mut rows = vec![json!({
            "path": path,
            "language": language,
            "parsed": body.parsed(cx),
        })];
        let range = |range: &std::ops::Range<text::Anchor>| {
            json!([
                at(range.start.to_point(&snapshot)),
                at(range.end.to_point(&snapshot))
            ])
        };
        for item in snapshot.outline(None).items {
            rows.push(json!([
                "item",
                item.depth,
                item.text.to_string(),
                range(&item.range),
                range(&item.selection_range),
                range(&item.source_range_for_text),
                item.body_range.as_ref().map(&range),
                item.name_ranges
                    .iter()
                    .map(|name| [name.start, name.end])
                    .collect::<Vec<_>>(),
            ]));
        }
        let last = snapshot.max_point().row;
        for top in 0..=last {
            let end = (top + STICKY_ROWS).min(last);
            let items = snapshot.outline_items_containing(
                Point::new(top, 0)..Point::new(end, 0),
                true,
                None,
            );
            let items: Vec<Value> = items
                .iter()
                .map(|item| {
                    json!([
                        item.depth,
                        item.text.to_string(),
                        at(item.selection_range.start.to_point(&snapshot)),
                        at(item.source_range_for_text.start.to_point(&snapshot)),
                        at(item.range.end.to_point(&snapshot)),
                    ])
                })
                .collect();
            rows.push(json!(["sticky", top, items]));
        }
        rows
    })
}

/// The indent guides of the whole file, then the active ones with the caret on each row.
fn guide_rows(body: &Entity<ZedBody>, cx: &mut VisualTestContext) -> Vec<Value> {
    let editor = body.read_with(cx, |body, _| body.editor().clone());
    let guides = editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        let last = snapshot.buffer_snapshot().max_row();
        editor
            .indent_guides(Default::default()..last, &snapshot, cx)
            .unwrap_or_default()
    });
    let mut rows: Vec<Value> = guides
        .iter()
        .map(|guide| {
            json!([
                "guide",
                guide.start_row.0,
                guide.end_row.0,
                guide.depth,
                guide.tab_size
            ])
        })
        .collect();
    let last = body.read_with(cx, |body, cx| body.buffer().read(cx).max_point().row);
    for row in 0..=last {
        body.update_in(cx, |body, window, cx| {
            body.go_to(row + 1, Some(1), window, cx)
        });
        cx.run_until_parked();
        // The first ask may finish in the background; the second one reads its answer.
        let mut active = Vec::new();
        for _ in 0..2 {
            active = editor.update_in(cx, |editor, window, cx| {
                let snapshot = editor.display_snapshot(cx);
                let found = editor.find_active_indent_guide_indices(&guides, &snapshot, window, cx);
                let mut found: Vec<usize> = found.into_iter().flatten().collect();
                found.sort_unstable();
                found
            });
            cx.run_until_parked();
        }
        rows.push(json!(["active", row, active]));
    }
    rows
}

fn rows(path: &str, text: &str, cx: &mut TestAppContext) -> Vec<Value> {
    let (body, cx) = cx.add_window_view(|window, cx| ZedBody::new(path, text, window, cx));
    cx.run_until_parked();
    let mut rows = outline_rows(path, &body, cx);
    rows.extend(guide_rows(&body, cx));
    rows
}

fn start(cx: &mut TestAppContext) {
    test_views::init(cx);
    cx.update(crate::git::languages::init);
}

#[gpui::test]
fn the_outline_and_the_guides_match_the_golden_files(cx: &mut TestAppContext) {
    start(cx);
    for (name, path, text) in files() {
        golden_files::check(SET, &name, &rows(&path, &text, cx));
    }
}

/// Fails when a shot file's rows miss what the shots draw: a sign that a guess about the old code
/// (the parse, the background answer of the active guide) is wrong. The old code takes a tab size
/// of 4, so the 2-space rows of the long file give no guide; the sticky file's 4-space rows give
/// one, active only with the caret on some rows.
fn assert_sane(name: &str, rows: &[Value]) {
    assert!(rows[0]["parsed"].as_bool().unwrap(), "{name}: not parsed");
    let has = |kind: &str| rows.iter().any(|row| row[0] == kind);
    assert!(has("item"), "{name}: no outline item");
    if name != "file-sticky" {
        return;
    }
    assert!(has("guide"), "{name}: no indent guide");
    let active: Vec<bool> = rows
        .iter()
        .filter(|row| row[0] == "active")
        .map(|row| !row[2].as_array().unwrap().is_empty())
        .collect();
    assert!(
        active.contains(&true),
        "{name}: no caret row has an active guide"
    );
    assert!(
        active.contains(&false),
        "{name}: every caret row has an active guide"
    );
}

/// Whether `name` is the long file or the sticky file of the editor shots.
fn is_shot_file(name: &str) -> bool {
    matches!(name, "file-long" | "file-sticky")
}

#[gpui::test]
fn the_shot_files_have_outline_items_and_an_active_guide(cx: &mut TestAppContext) {
    start(cx);
    for (name, path, text) in files().into_iter().filter(|(name, ..)| is_shot_file(name)) {
        assert_sane(&name, &rows(&path, &text, cx));
    }
}

/// Writes nothing wrong: a shot file whose rows fail `assert_sane` stops the run.
#[gpui::test]
#[ignore]
fn golden_editor_outline(cx: &mut TestAppContext) {
    start(cx);
    for (name, path, text) in files() {
        let rows = rows(&path, &text, cx);
        if is_shot_file(&name) {
            assert_sane(&name, &rows);
        }
        golden_files::write(SET, &name, &rows);
    }
}
