#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use super::*;
use crate::app::golden_files;
use crate::app::shots_text::{LONG_FILE, STICKY_FILE, long_file, sticky_file};
use crate::git::languages::golden_tests::fixtures;
use crate::git::languages::{LANGS, lang_for};
use crate::text_view::code_langs::typing_for;

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
    files.push((
        "file-sticky".to_owned(),
        STICKY_FILE.to_owned(),
        sticky_file(),
    ));
    let mut problems = Vec::new();
    for (name, _, text) in files {
        let golden: Vec<Value> = golden_files::read("outline", &name)
            .into_iter()
            .filter(|row| row[0] == "guide" || row[0] == "active")
            .collect();
        let own = guide_rows(&text);
        if let Some(at) = (0..golden.len().max(own.len())).find(|&at| golden.get(at) != own.get(at))
        {
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
    let none = Scope::default();
    let string = Scope {
        string: true,
        comment: false,
    };
    assert_eq!(json.closer("", 0, '{', none), Some("}"));
    assert_eq!(json.closer("x", 0, '(', none), None);
    assert_eq!(json.closer(",", 0, '[', none), Some("]"));
    assert_eq!(json.closer("x", 1, '"', none), None);
    assert_eq!(json.closer("x", 1, '(', none), Some(")"));
    assert_eq!(json.closer("", 0, '\'', none), None);
    assert_eq!(json.closer("", 0, '"', string), None);
    let text = "{\"a\": \"ab\"}";
    assert!(json.scope_at(&json_tree(text), text, 9).string);
    assert!(!json.scope_at(&json_tree(text), text, 10).string);
    assert!(json.expands("{}", 1));
    assert!(!json.expands("()", 1));
}

#[test]
fn a_pair_whose_opening_has_more_chars_closes_once_its_last_char_is_typed() {
    let rust = typing_for("Rust").unwrap();
    let none = Scope::default();
    assert_eq!(rust.closer("/", 1, '*', none), Some(" */"));
    assert_eq!(rust.closer("/*", 2, '*', none), None);
    assert_eq!(rust.closer("r#", 2, '"', none), Some("\"#"));
    let comment = Scope {
        string: false,
        comment: true,
    };
    assert_eq!(rust.closer("/", 1, '*', comment), None);
    assert_eq!(rust.closer("", 0, '(', comment), Some(")"));
    let python = typing_for("Python").unwrap();
    assert_eq!(python.closer("f", 1, '"', none), Some("\""));
    assert_eq!(python.closer("x", 1, '"', none), None);
}

#[test]
fn a_line_comment_goes_on_with_the_longest_start_of_the_line() {
    let rust = typing_for("Rust").unwrap();
    assert_eq!(rust.line_comment("    // a"), Some("// "));
    assert_eq!(rust.line_comment("/// a"), Some("/// "));
    assert_eq!(rust.line_comment("let a = 1; // a"), None);
    assert_eq!(
        typing_for("Python").unwrap().line_comment("# a"),
        Some("# ")
    );
}

#[test]
fn every_language_has_typing_facts() {
    for lang in &LANGS {
        assert!(typing_for(lang.name).is_some(), "{}", lang.name);
    }
    assert!(typing_for(lang_for(".natamux.json").unwrap().name).is_some());
}

#[test]
fn the_selected_text_is_found_whatever_its_case() {
    assert_eq!(
        find_all("name Name NAME nam", "name"),
        vec![0..4, 5..9, 10..14]
    );
    assert_eq!(find_all("aaaa", "aa"), vec![0..2, 2..4]);
    // An ASCII query matches ASCII letters only: the Kelvin sign is not a `k`.
    assert_eq!(find_all("\u{212a} k K", "k"), vec![4..5, 6..7]);
    assert_eq!(find_all("Été été ÉTÉ", "été"), vec![0..5, 6..11, 12..17]);
    assert!(find_all("abc", "x").is_empty());
}
