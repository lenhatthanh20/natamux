#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use super::*;
use crate::app::golden_files;
use crate::app::shots_text::{LONG_FILE, STICKY_FILE, long_file, sticky_file};
use crate::git::languages::golden_tests::fixtures;
use crate::git::languages::lang_for;

fn at(text: &str, offset: usize) -> Value {
    let before = &text.as_bytes()[..offset];
    let row = before.iter().filter(|byte| **byte == b'\n').count();
    let start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    json!([row, offset - start])
}

fn span(text: &str, range: &Range<usize>) -> Value {
    json!([at(text, range.start), at(text, range.end)])
}

fn items_of(lang: &'static Lang, text: &str) -> Vec<OutlineItem> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).unwrap();
    let tree = parser.parse(text, None).unwrap();
    outline(lang, &tree, text)
}

/// The `item` rows of an outline golden file (T-OUT-1, Zed's editor), as the own outline gives
/// them.
fn item_rows(lang: &'static Lang, text: &str) -> Vec<Value> {
    items_of(lang, text)
        .iter()
        .map(|item| {
            json!([
                "item",
                item.depth,
                item.text,
                span(text, &item.range),
                span(text, &item.selection),
                span(text, &item.source),
                item.body.as_ref().map(|body| span(text, body)),
                item.names
                    .iter()
                    .map(|name| [name.start, name.end])
                    .collect::<Vec<_>>(),
            ])
        })
        .collect()
}

/// The rows sticky scroll looks at below the top row (as in the golden generator).
const STICKY_ROWS: usize = 10;

/// The `sticky` rows of an outline golden file: for each top row, the items that meet the rows
/// from it to 10 rows below (the last row at most), as the own outline gives them.
fn sticky_rows(items: &[OutlineItem], text: &str) -> Vec<Value> {
    let mut line_starts = vec![0];
    line_starts.extend(
        text.bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(at, _)| at + 1),
    );
    let last = line_starts.len() - 1;
    (0..=last)
        .map(|top| {
            let end = (top + STICKY_ROWS).min(last);
            let found: Vec<Value> = containing(items, line_starts[top]..line_starts[end])
                .iter()
                .map(|item| {
                    json!([
                        item.depth,
                        item.text,
                        at(text, item.selection.start),
                        at(text, item.source.start),
                        at(text, item.range.end),
                    ])
                })
                .collect();
            json!(["sticky", top, found])
        })
        .collect()
}

/// Every file of the outline set, with its name and path.
fn outline_files() -> Vec<(String, String, String)> {
    let mut files = fixtures();
    files.push(("file-long".to_owned(), LONG_FILE.to_owned(), long_file()));
    files.push((
        "file-sticky".to_owned(),
        STICKY_FILE.to_owned(),
        sticky_file(),
    ));
    files
}

/// The first row where `own` and `golden` differ, as a line for the failure message.
fn first_difference(name: &str, golden: &[Value], own: &[Value]) -> Option<String> {
    let at = (0..golden.len().max(own.len())).find(|&at| golden.get(at) != own.get(at))?;
    Some(format!(
        "{name}: row {at} of {}/{}\n old: {}\n own: {}",
        golden.len(),
        own.len(),
        golden.get(at).map_or("none".to_owned(), Value::to_string),
        own.get(at).map_or("none".to_owned(), Value::to_string),
    ))
}

#[test]
fn every_outline_query_compiles() {
    let problems: Vec<String> = crate::git::languages::LANGS
        .iter()
        .filter_map(query_problem)
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The outline items match the old editor's for every file of the outline set.
#[test]
fn the_items_match_the_outline_golden_files() {
    let mut problems = Vec::new();
    for (name, path, text) in outline_files() {
        let golden: Vec<Value> = golden_files::read("outline", &name)
            .into_iter()
            .filter(|row| row[0] == "item")
            .collect();
        let own = lang_for(&path).map_or_else(Vec::new, |lang| item_rows(lang, &text));
        problems.extend(first_difference(&name, &golden, &own));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The items sticky scroll asks for at each top row match the old editor's (T-OUT-1 `sticky`).
#[test]
fn the_sticky_items_match_the_outline_golden_files() {
    let mut problems = Vec::new();
    for (name, path, text) in outline_files() {
        let golden: Vec<Value> = golden_files::read("outline", &name)
            .into_iter()
            .filter(|row| row[0] == "sticky")
            .collect();
        let items = lang_for(&path).map_or_else(Vec::new, |lang| items_of(lang, &text));
        problems.extend(first_difference(&name, &golden, &sticky_rows(&items, &text)));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// An item that meets the rows only at its edge is not in them, and depths count only the found
/// items around an item.
#[test]
fn containing_takes_the_items_that_meet_the_range() {
    let item = |range: Range<usize>| OutlineItem {
        depth: 9,
        text: String::new(),
        selection: range.clone(),
        source: range.clone(),
        range,
        body: None,
        names: Vec::new(),
    };
    let items = [item(0..10), item(2..4), item(5..9), item(10..20)];
    let found = containing(&items, 4..10);
    let ranges: Vec<(Range<usize>, usize)> = found
        .iter()
        .map(|item| (item.range.clone(), item.depth))
        .collect();
    assert_eq!(ranges, vec![(0..10, 0), (5..9, 1)]);
    let inner = containing(&items[1..], 0..5);
    assert_eq!(inner.len(), 1);
    assert_eq!(inner[0].depth, 0);
}

#[test]
fn a_language_with_no_query_has_no_items() {
    let lang = lang_for("deploy.sh").unwrap();
    assert!(item_rows(lang, "echo hi\n").is_empty());
}
