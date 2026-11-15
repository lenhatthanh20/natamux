#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use super::*;
use crate::app::golden_files;
use crate::app::shots_text::{LONG_FILE, STICKY_FILE, long_file, sticky_file};
use crate::git::languages::golden_tests::fixtures;
use crate::git::languages::{LANGS, lang_for};

/// The `guide` and `active` rows of an outline golden file (T-OUT-1, Zed's editor), as the own
/// rules give them.
fn guide_rows(text: &str) -> Vec<Value> {
    let found = guides(text);
    let mut rows: Vec<Value> = found
        .iter()
        .map(|guide| json!(["guide", guide.start, guide.end, guide.depth, TAB_SIZE]))
        .collect();
    let last = text.split('\n').count() - 1;
    for row in 0..=last {
        rows.push(json!(["active", row, active_guides(text, &found, row)]));
    }
    rows
}

/// The guides and the active guide match the old editor's for every file of the outline set.
#[test]
fn the_guides_match_the_outline_golden_files() {
    let mut files = fixtures();
    files.push(("file-long".to_owned(), LONG_FILE.to_owned(), long_file()));
    files.push(("file-sticky".to_owned(), STICKY_FILE.to_owned(), sticky_file()));
    let mut problems = Vec::new();
    for (name, _, text) in files {
        let golden: Vec<Value> = golden_files::read("outline", &name)
            .into_iter()
            .filter(|row| row[0] == "guide" || row[0] == "active")
            .collect();
        let own = guide_rows(&text);
        if let Some(at) = (0..golden.len().max(own.len())).find(|&at| golden.get(at) != own.get(at)) {
            problems.push(format!(
                "{name}: row {at}: old {:?}, own {:?}",
                golden.get(at),
                own.get(at)
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn a_blank_line_takes_the_level_of_the_line_below() {
    let text = "a\n\n    b\n    c\n\nd";
    assert_eq!(
        guides(text),
        vec![Guide {
            start: 1,
            end: 3,
            depth: 0
        }]
    );
}

fn json_tree(text: &str) -> tree_sitter::Tree {
    let lang = LANGS.iter().find(|lang| lang.name == "JSON").unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&(lang.grammar)()).unwrap();
    parser.parse(text, None).unwrap()
}

#[test]
fn a_new_line_in_an_object_goes_one_level_deeper_than_its_first_row() {
    let json = typing_for("JSON").unwrap();
    let text = "{\n  \"a\": {\n\n  }\n}";
    assert_eq!(json.indent_for(text, &json_tree(text), 2), "      ");
    let text = "    a\n";
    assert_eq!(json.indent_for(text, &json_tree(text), 1), "    ");
    let text = "{\n\n}";
    assert_eq!(json.indent_for(text, &json_tree(text), 2), "");
}

#[test]
fn json_closes_brackets_and_quotes_only_where_the_old_editor_did() {
    let json = typing_for("JSON").unwrap();
    assert_eq!(json.closer("", 0, '{', false), Some('}'));
    assert_eq!(json.closer("x", 0, '(', false), None);
    assert_eq!(json.closer(",", 0, '[', false), Some(']'));
    assert_eq!(json.closer("x", 1, '"', false), None);
    assert_eq!(json.closer("x", 1, '(', false), Some(')'));
    assert_eq!(json.closer("", 0, '\'', false), None);
    assert_eq!(json.closer("", 0, '"', true), None);
    let text = "{\"a\": \"ab\"}";
    assert!(json.in_string(&json_tree(text), 9));
    assert!(!json.in_string(&json_tree(text), 10));
    assert!(json.expands("{}", 1));
    assert!(!json.expands("()", 1));
}

#[test]
fn only_json_has_typing_facts_so_far() {
    assert!(typing_for(lang_for(".natamux.json").unwrap().name).is_some());
    assert!(typing_for(lang_for("x.rs").unwrap().name).is_none());
}
