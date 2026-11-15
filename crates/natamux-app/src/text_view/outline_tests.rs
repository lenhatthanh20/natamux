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

/// The `item` rows of an outline golden file (T-OUT-1, Zed's editor), as the own outline gives
/// them.
fn item_rows(lang: &'static Lang, text: &str) -> Vec<Value> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).unwrap();
    let tree = parser.parse(text, None).unwrap();
    outline(lang, &tree, text)
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
    let mut files = fixtures();
    files.push(("file-long".to_owned(), LONG_FILE.to_owned(), long_file()));
    files.push((
        "file-sticky".to_owned(),
        STICKY_FILE.to_owned(),
        sticky_file(),
    ));
    let mut problems = Vec::new();
    for (name, path, text) in files {
        let golden: Vec<Value> = golden_files::read("outline", &name)
            .into_iter()
            .filter(|row| row[0] == "item")
            .collect();
        let own = lang_for(&path).map_or_else(Vec::new, |lang| item_rows(lang, &text));
        if let Some(at) = (0..golden.len().max(own.len())).find(|&at| golden.get(at) != own.get(at))
        {
            problems.push(format!(
                "{name}: row {at} of {}/{}\n old: {}\n own: {}",
                golden.len(),
                own.len(),
                golden.get(at).map_or("none".to_owned(), Value::to_string),
                own.get(at).map_or("none".to_owned(), Value::to_string),
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_language_with_no_query_has_no_items() {
    let lang = lang_for("deploy.sh").unwrap();
    assert!(item_rows(lang, "echo hi\n").is_empty());
}
