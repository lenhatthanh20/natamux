//! T-DIFF-1 (plan 6.5, 7.6): what the diff view works out on today's code, kept as golden files
//! for the own diff model. Per case: the hunks, the word diffs, and the rows of each side in four
//! modes (side by side and inline, with the whole file and with unchanged lines collapsed). The
//! cases are the 6 shapes of `diff_view_tests.rs` (each with 4 final-break combinations), word-diff
//! fixtures in JS, CSS and JSON, and 20 seeded random edits.
//!
//! Word diffs are recorded twice for the fixtures: as the app gets them (the language is set on
//! the buffers after the diff is started, so it may not count), and with the language set first.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use buffer_diff::{BufferDiff, DiffHunkStatusKind};
use editor::Editor;
use editor::display_map::DisplayRow;
use gpui::{Entity, TestAppContext, VisualTestContext};
use language::Buffer;
use serde_json::{Value, json};
use text::ToOffset;
use ui::prelude::*;

use super::{DiffOptions, DiffView};
use crate::app::{golden_files, test_views};
use crate::git::diff_cases::{cases, random_cases, word_fixtures};
use crate::git::languages::{self, language_for};

const SET: &str = "diff";

/// The four ways the diff is drawn: `(name, side by side, collapse)`.
const MODES: [(&str, bool, bool); 4] = [
    ("side whole", true, false),
    ("side collapsed", true, true),
    ("inline whole", false, false),
    ("inline collapsed", false, true),
];

fn open(cx: &mut TestAppContext) -> (Entity<DiffView>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.update(languages::init);
    cx.add_window_view(|_, _| DiffView::new())
}

fn show(
    view: &Entity<DiffView>,
    key: &str,
    path: &str,
    (original, modified): &(String, String),
    (side_by_side, collapse): (bool, bool),
    cx: &mut VisualTestContext,
) {
    view.update_in(cx, |view, window, cx| {
        view.show(
            key,
            path,
            1,
            Some(original),
            modified,
            DiffOptions {
                side_by_side,
                collapse,
                wrap: false,
                ignore_whitespace: false,
            },
            window,
            cx,
        )
    });
    cx.run_until_parked();
}

/// The rows of one editor as text: a letter per row (L unchanged, A added, D deleted, M modified,
/// B a block such as an empty row) and the 1-based buffer line when the row has one. Runs of
/// rows with consecutive lines are one item, `L1-277`; a run of rows without a line is `B*3`.
fn rows_of(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
    editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        let count = snapshot.max_point().row().0 as usize + 1;
        // (letter, first line, last line, rows)
        let mut runs: Vec<(char, Option<u32>, Option<u32>, u32)> = Vec::new();
        for (row, info) in snapshot.row_infos(DisplayRow(0)).take(count).enumerate() {
            let letter = if snapshot.is_block_line(DisplayRow(row as u32)) {
                'B'
            } else {
                match info.diff_status.map(|status| status.kind) {
                    Some(DiffHunkStatusKind::Added) => 'A',
                    Some(DiffHunkStatusKind::Deleted) => 'D',
                    Some(DiffHunkStatusKind::Modified) => 'M',
                    None => 'L',
                }
            };
            let line = info.buffer_row.map(|row| row + 1);
            match runs.last_mut() {
                Some((last, _, end, rows))
                    if *last == letter
                        && match (*end, line) {
                            (Some(end), Some(line)) => line == end + 1,
                            (None, None) => true,
                            _ => false,
                        } =>
                {
                    *end = line;
                    *rows += 1;
                }
                _ => runs.push((letter, line, line, 1)),
            }
        }
        let items: Vec<String> = runs
            .iter()
            .map(|(letter, first, last, rows)| match (first, last) {
                (Some(first), Some(last)) if first == last => format!("{letter}{first}"),
                (Some(first), Some(last)) => format!("{letter}{first}-{last}"),
                _ => format!("{letter}*{rows}"),
            })
            .collect();
        items.join(" ")
    })
}

/// The word diffs of the hunks of `diff` over `buffer`: per hunk its new rows, then the changed
/// words of the old side and of the new side as `[start, end, text]` (offsets relative to the
/// hunk's old text for the old side, in the buffer for the new side).
fn words(diff: &Entity<BufferDiff>, buffer: &Entity<Buffer>, cx: &App) -> Value {
    let text = buffer.read(cx).text_snapshot();
    let new_text = buffer.read(cx).text();
    let base_buffer = diff.read(cx).base_text_buffer().clone();
    let base_text = base_buffer.read(cx).text();
    let snapshot = diff.read(cx).snapshot(cx);
    let hunks: Vec<Value> = snapshot
        .hunks(&text)
        .map(|hunk| {
            let from = hunk.diff_base_byte_range.start;
            let old: Vec<Value> = hunk
                .base_word_diffs
                .iter()
                .map(|r| json!([r.start, r.end, base_text.get(from + r.start..from + r.end)]))
                .collect();
            let new: Vec<Value> = hunk
                .buffer_word_diffs
                .iter()
                .map(|r| {
                    let (start, end) = (r.start.to_offset(&text), r.end.to_offset(&text));
                    json!([start, end, new_text.get(start..end)])
                })
                .collect();
            json!([hunk.range.start.row, hunk.range.end.row, old, new])
        })
        .collect();
    Value::Array(hunks)
}

/// The word diffs when the base buffer has its language before the diff starts.
fn words_language_first(
    path: &str,
    (original, modified): &(String, String),
    cx: &mut VisualTestContext,
) -> Value {
    let (diff, buffer, loaded) = cx.update(|_, cx| {
        let buffer = cx.new(|cx| Buffer::local(modified.as_str(), cx));
        let snapshot = buffer.read(cx).text_snapshot();
        let diff = cx.new(|cx| BufferDiff::new(&snapshot, None, None, cx));
        let base = diff.read(cx).base_text_buffer().clone();
        let language = language_for(path, cx).expect("the languages are registered");
        let loaded = base.clone();
        cx.spawn(async move |cx| {
            if let Some(language) = language.await {
                base.update(cx, |base, cx| base.set_language(Some(language), cx));
            }
        })
        .detach();
        (diff, buffer, loaded)
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            loaded.read(cx).language().is_some(),
            "{path}: the language did not load"
        );
        let snapshot = buffer.read(cx).text_snapshot();
        let base: Arc<str> = Arc::from(original.as_str());
        diff.update(cx, |diff, cx| diff.set_base_text(Some(base), snapshot, cx))
            .detach();
    });
    cx.run_until_parked();
    cx.update(|_, cx| words(&diff, &buffer, cx))
}

/// The hunks as `[old start, old end, new start, new end]`.
fn hunks(view: &Entity<DiffView>, cx: &mut VisualTestContext) -> Value {
    view.read_with(cx, |view, _| {
        Value::Array(
            view.hunks
                .iter()
                .map(|hunk| json!([hunk.old.start, hunk.old.end, hunk.new.start, hunk.new.end]))
                .collect(),
        )
    })
}

fn app_words(view: &Entity<DiffView>, cx: &mut VisualTestContext) -> Value {
    view.read_with(cx, |view, cx| {
        let diff = view.diff.clone().expect("a diff");
        let buffer = view.buffer.clone().expect("a buffer");
        words(&diff, &buffer, cx)
    })
}

/// Records one case: hunks, the app's word diffs, and the rows of each side in every mode.
fn record(
    rows: &mut Vec<Value>,
    view: &Entity<DiffView>,
    name: &str,
    path: &str,
    texts: &(String, String),
    modes: &[(&str, bool, bool)],
    cx: &mut VisualTestContext,
) {
    for (i, (mode, side_by_side, collapse)) in modes.iter().enumerate() {
        show(
            view,
            &format!("{name} {mode}"),
            path,
            texts,
            (*side_by_side, *collapse),
            cx,
        );
        if i == 0 {
            rows.push(json!([name, "hunks", hunks(view, cx)]));
            rows.push(json!([name, "words", app_words(view, cx)]));
        }
        let (left, right) = view.read_with(cx, |view, _| {
            let sides = view.sides.as_ref().expect("the diff is drawn");
            (sides.left.clone(), sides.right.clone())
        });
        if let Some(left) = left {
            rows.push(json!([name, mode, "left", rows_of(&left, cx)]));
        }
        rows.push(json!([name, mode, "right", rows_of(&right, cx)]));
    }
}

fn run(cx: &mut TestAppContext) -> Vec<Value> {
    let (view, cx) = open(cx);
    let mut rows = Vec::new();
    for (name, texts) in cases() {
        record(&mut rows, &view, &name, "notes.txt", &texts, &MODES, cx);
    }
    for (name, path, texts) in word_fixtures() {
        record(&mut rows, &view, &name, path, &texts, &MODES[..1], cx);
        let first = words_language_first(path, &texts, cx);
        rows.push(json!([name, "words, language first", first]));
    }
    for (name, path, texts) in random_cases() {
        record(
            &mut rows,
            &view,
            &name,
            path,
            &texts,
            &[MODES[1], MODES[2]],
            cx,
        );
        let first = words_language_first(path, &texts, cx);
        rows.push(json!([name, "words, language first", first]));
    }
    rows
}

#[gpui::test]
fn the_diff_matches_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, "rows", &run(cx));
}

#[gpui::test]
#[ignore]
fn golden_diff(cx: &mut TestAppContext) {
    golden_files::write(SET, "rows", &run(cx));
}
