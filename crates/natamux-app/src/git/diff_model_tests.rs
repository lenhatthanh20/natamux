//! The own diff model against T-DIFF-1 (the rows the old view gave, `diff_golden_tests.rs`), and
//! the small rules it is made of.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use super::*;
use crate::app::golden_files;
use crate::git::diff_cases::{cases, random_cases, word_fixtures};

/// `(name, side by side, collapse)`, as in `diff_golden_tests.rs`.
const MODES: [(&str, bool, bool); 4] = [
    ("side whole", true, false),
    ("side collapsed", true, true),
    ("inline whole", false, false),
    ("inline collapsed", false, true),
];

/// The rows in the golden file's text: a letter per row (L unchanged, A added, D deleted, B an
/// empty row) with the 1-based line; runs of consecutive lines joined, `B*3` for rows with none.
fn rows_text(rows: &[Row]) -> String {
    // (letter, first line, last line, rows)
    let mut runs: Vec<(char, Option<u32>, Option<u32>, u32)> = Vec::new();
    for row in rows {
        let (letter, line, count) = match *row {
            Row::Line { row, status } => {
                let letter = match status {
                    Status::Unchanged => 'L',
                    Status::Added => 'A',
                    Status::Deleted => 'D',
                };
                (letter, Some(row + 1), 1)
            }
            Row::Deleted { row } => ('D', Some(row + 1), 1),
            Row::Spacer { rows } => ('B', None, rows),
            Row::Gap { .. } => ('B', None, 1),
        };
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
                *rows += count;
            }
            _ => runs.push((letter, line, line, count)),
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
}

fn hunks_json(diff: &FileDiff) -> Value {
    Value::Array(
        diff.hunks
            .iter()
            .map(|hunk| {
                let rows = &hunk.rows;
                json!([rows.old.start, rows.old.end, rows.new.start, rows.new.end])
            })
            .collect(),
    )
}

/// Per hunk: its new rows as the old editor gave them (a row past the end is the last one), then
/// the changed words of each side as `[start, end, text]`.
fn words_json(diff: &FileDiff) -> Value {
    let last = diff.new.matches('\n').count() as u32;
    Value::Array(
        diff.hunks
            .iter()
            .map(|hunk| {
                let from = diff.old_start(hunk);
                let old: Vec<Value> = hunk
                    .old_words
                    .iter()
                    .map(|r| json!([r.start, r.end, diff.old.get(from + r.start..from + r.end)]))
                    .collect();
                let new: Vec<Value> = hunk
                    .new_words
                    .iter()
                    .map(|r| json!([r.start, r.end, diff.new.get(r.start..r.end)]))
                    .collect();
                let rows = &hunk.rows.new;
                json!([rows.start.min(last), rows.end.min(last), old, new])
            })
            .collect(),
    )
}

fn record(
    rows: &mut Vec<Value>,
    name: &str,
    path: &str,
    (original, modified): &(String, String),
    modes: &[(&str, bool, bool)],
) -> FileDiff {
    let diff = diff(Some(original), modified, word_chars(path));
    rows.push(json!([name, "hunks", hunks_json(&diff)]));
    rows.push(json!([name, "words", words_json(&diff)]));
    let hunks = diff.hunk_rows();
    let (old_rows, new_rows) = (diff.old_rows(), diff.new_rows());
    for (mode, side_by_side, collapse) in modes {
        if *side_by_side {
            let left = side_rows(&hunks, Side::Old, old_rows, new_rows, *collapse);
            let right = side_rows(&hunks, Side::New, old_rows, new_rows, *collapse);
            rows.push(json!([name, mode, "left", rows_text(&left)]));
            rows.push(json!([name, mode, "right", rows_text(&right)]));
        } else {
            let right = inline_rows(&hunks, new_rows, *collapse);
            rows.push(json!([name, mode, "right", rows_text(&right)]));
        }
    }
    diff
}

/// The rows of `diff_golden_tests.rs::run`, in its order, from the own model.
fn run() -> Vec<Value> {
    let mut rows = Vec::new();
    for (name, texts) in cases() {
        record(&mut rows, &name, "notes.txt", &texts, &MODES);
    }
    // The old view's word diffs did not depend on when the language came; both rows are equal.
    for (name, path, texts) in word_fixtures() {
        let diff = record(&mut rows, &name, path, &texts, &MODES[..1]);
        rows.push(json!([name, "words, language first", words_json(&diff)]));
    }
    for (name, path, texts) in random_cases() {
        let diff = record(&mut rows, &name, path, &texts, &[MODES[1], MODES[2]]);
        rows.push(json!([name, "words, language first", words_json(&diff)]));
    }
    rows
}

#[test]
fn the_own_model_matches_the_diff_golden_file() {
    golden_files::check("diff", "rows", &run());
}

#[test]
fn a_punctuation_token_holds_one_repeated_char() {
    assert_eq!(tokens("a == b", &[]), ["a", " ", "==", " ", "b"]);
    assert_eq!(tokens("x=>y", &[]), ["x", "=", ">", "y"]);
    assert_eq!(tokens("a::b-c", &[]), ["a", "::", "b", "-", "c"]);
    assert_eq!(tokens("a::b-c", &['-']), ["a", "::", "b-c"]);
    assert_eq!(tokens("", &[]), Vec::<&str>::new());
}

#[test]
fn css_and_markdown_count_the_dash_as_a_word_char() {
    assert_eq!(word_chars("a.css"), ['-', '@']);
    assert_eq!(word_chars("README.md"), ['-']);
    assert!(word_chars("a.js").is_empty());
    assert!(word_chars("notes.txt").is_empty());
}

#[test]
fn a_change_of_every_token_is_one_range() {
    let (old, new) = word_diff("a.b", "x,y", &[]);
    assert_eq!((old.len(), old.first()), (1, Some(&(0..3))));
    assert_eq!((new.len(), new.first()), (1, Some(&(0..3))));
}

#[test]
fn line_endings_become_line_breaks() {
    assert_eq!(normalize("a\r\nb\rc\n"), "a\nb\nc\n");
    let diff = diff(Some("a\r\nb\r\n"), "a\nb\n", &[]);
    assert!(diff.hunks.is_empty());
}

#[test]
fn an_emptied_file_is_one_hunk_of_every_old_line() {
    let diff = diff(Some("a\nb\n"), "\n", &[]);
    assert_eq!(diff.hunk_rows(), [HunkRows { old: 0..2, new: 0..0 }]);
    let diff = super::diff(Some("a\nb\n\n"), "\n", &[]);
    assert_eq!(diff.hunk_rows(), [HunkRows { old: 0..2, new: 0..0 }]);
}

#[test]
fn a_new_file_is_one_hunk_of_every_line() {
    let diff = diff(None, "a\nb\n", &[]);
    assert_eq!(diff.hunk_rows(), [HunkRows { old: 0..0, new: 0..2 }]);
    let diff = super::diff(None, "a\nb", &[]);
    assert_eq!(diff.hunk_rows(), [HunkRows { old: 0..0, new: 0..2 }]);
    assert_eq!(super::diff(None, "", &[]).hunk_rows()[0].new, 0..0);
}

#[test]
fn a_word_diff_needs_the_same_line_count_on_both_sides() {
    let diff = diff(Some("a\nold x\nb\n"), "a\nnew x\nb\n", &[]);
    let hunk = &diff.hunks[0];
    assert_eq!((hunk.old_words.len(), hunk.old_words.first()), (1, Some(&(0..3))));
    assert_eq!((hunk.new_words.len(), hunk.new_words.first()), (1, Some(&(2..5))));
    let diff = super::diff(Some("a\nold\nb\n"), "a\nnew\nmore\nb\n", &[]);
    assert!(diff.hunks[0].old_words.is_empty() && diff.hunks[0].new_words.is_empty());
}

#[test]
fn parts_that_touch_are_joined() {
    // 0..=7 and 8..=15 touch.
    let joined = parts(&[3..4, 11..12], 30, true);
    assert_eq!((joined.len(), joined.first()), (1, Some(&(0..=15))));
    assert_eq!(parts(&[3..4, 12..13], 30, true), [0..=7, 9..=16]);
    let one = |range: Range<u32>| parts(std::slice::from_ref(&range), 30, true);
    assert_eq!(one(28..30).first(), Some(&(25..=29)));
    assert_eq!(parts(std::slice::from_ref(&(3..4)), 30, false).first(), Some(&(0..=29)));
    assert_eq!(parts(&[], 30, true).first(), Some(&(0..=29)));
}

#[test]
fn a_gap_counts_the_rows_it_hides() {
    let hunks = [HunkRows {
        old: 20..21,
        new: 20..21,
    }];
    let rows = side_rows(&hunks, Side::New, 40, 40, true);
    assert_eq!(rows.first(), Some(&Row::Line { row: 17, status: Status::Unchanged }));
    let hunks = [
        HunkRows { old: 5..6, new: 5..6 },
        HunkRows {
            old: 20..21,
            new: 20..21,
        },
    ];
    let rows = inline_rows(&hunks, 40, true);
    assert!(rows.contains(&Row::Gap { hidden: 7 }));
}

#[test]
fn the_plain_line_diff_gives_the_changed_rows() {
    assert_eq!(line_diff("a\nb\nc\n", "a\nx\nc\n"), [(1..2, 1..2)]);
    assert!(line_diff("a\n", "a\n").is_empty());
}
