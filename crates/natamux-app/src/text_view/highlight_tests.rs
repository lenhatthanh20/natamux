#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use gpui::TestAppContext;
use serde_json::{Value, json};
use theme::{ActiveTheme, SyntaxTheme};
use tree_sitter::{InputEdit, Point};

use super::*;
use crate::app::{golden_files, test_views};
use crate::git::languages::golden_tests::{fixtures, hex};
use crate::git::languages::lang_for;

/// The rows of T-HL-1 (`git/languages_golden_tests.rs`) for one file, from the own highlighter.
pub(super) fn rows(path: &str, text: &str, syntax_theme: &SyntaxTheme) -> Vec<Value> {
    let lang = lang_for(path).unwrap_or_else(|| panic!("{path}: no language"));
    let mut syntax = Syntax::new(lang).unwrap();
    let parsed = syntax.parse(text, Duration::from_secs(60));
    let mut rows = vec![json!({"path": path, "language": lang.name, "parsed": parsed})];
    let highlights = Highlights::of(lang).unwrap();
    let keys = highlights.theme_keys(|key| syntax_theme.style_for_name(key).is_some());
    let tree = syntax.tree().unwrap();
    for (range, key) in highlights.runs(tree, text, 0..text.len(), &keys) {
        let style = syntax_theme.style_for_name(&key).unwrap_or_default();
        rows.push(json!([
            range.start,
            range.end,
            key,
            style.color.map(hex),
            style.font_weight.map(|weight| weight.0),
            style.font_style.map(|style| format!("{style:?}")),
        ]));
    }
    rows
}

fn syntax_theme(cx: &mut TestAppContext) -> SyntaxTheme {
    test_views::init(cx);
    cx.update(|cx| (**cx.theme().syntax()).clone())
}

/// T-HL-1: the own highlighter gives the colours the old editor gave, for every fixture.
#[gpui::test]
fn the_colours_match_the_old_editor(cx: &mut TestAppContext) {
    let syntax_theme = syntax_theme(cx);
    let problems: Vec<String> = fixtures()
        .into_iter()
        .filter_map(|(name, path, text)| {
            golden_files::differs("highlight", &name, &rows(&path, &text, &syntax_theme))
        })
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_query_compiles() {
    for lang in &LANGS {
        if let Err(error) = Highlights::of(lang) {
            panic!("{error}");
        }
    }
}

#[test]
fn a_language_outside_the_table_is_refused() {
    let lang: &'static Lang = Box::leak(Box::new(Lang {
        name: "test",
        suffixes: &[],
        grammar: || tree_sitter_json::LANGUAGE.into(),
        queries: &[],
        renames: &[],
    }));
    assert!(Highlights::of(lang).is_err());
    assert!(Highlights::of(&LANGS[0]).is_ok());
}

#[test]
fn a_path_finds_its_language_as_the_old_registry_did() {
    let name = |path| lang_for(path).map(|lang| lang.name);
    assert_eq!(name("src/main.rs"), Some("Rust"));
    assert_eq!(name("a/ring.c"), Some("C++"));
    assert_eq!(name("a/b.H"), Some("C++"));
    assert_eq!(name(".env"), Some("Env"));
    assert_eq!(name("fixtures/prod.env"), Some("Env"));
    assert_eq!(name("go.mod"), Some("Go Mod"));
    assert_eq!(name("go.work"), Some("Go Work"));
    assert_eq!(name("x/COMMIT_EDITMSG"), Some("Git Commit"));
    assert_eq!(name("cart.js"), Some("JavaScript"));
    assert_eq!(name("Cart.tsx"), Some("TSX"));
    assert_eq!(name("README.MD"), Some("Markdown"));
    assert_eq!(name("a.json"), Some("JSON"));
    assert_eq!(name("a.jsonc"), Some("JSONC"));
    assert_eq!(name("notes.txt"), None);
    assert_eq!(name("Makefile"), None);
}

#[test]
fn a_name_takes_the_longest_theme_key_before_a_dot() {
    let keys = ["string", "string.special", "punctuation"];
    let has = |key: &str| keys.contains(&key);
    assert_eq!(
        theme_key("string.special.key", has).as_deref(),
        Some("string.special")
    );
    assert_eq!(theme_key("string.escape", has).as_deref(), Some("string"));
    assert_eq!(theme_key("string", has).as_deref(), Some("string"));
    assert_eq!(theme_key("strings", has), None);
    assert_eq!(theme_key("keyword", has), None);
}

/// Runs of a small JSON text with an own query, keyed by capture name.
fn json_runs(query: &'static str, text: &str) -> Vec<(usize, usize, String)> {
    let lang: &'static Lang = Box::leak(Box::new(Lang {
        name: "test",
        suffixes: &[],
        grammar: || tree_sitter_json::LANGUAGE.into(),
        queries: Box::leak(Box::new([crate::git::languages::QueryFile {
            text: query,
            first_wins: false,
        }])),
        renames: &[("dropped", None), ("renamed", Some("number"))],
    }));
    let highlights = compile(lang).unwrap();
    let keys = highlights.theme_keys(|key| key != "unknown");
    let mut syntax = Syntax::new(lang).unwrap();
    assert!(syntax.parse(text, Duration::from_secs(60)));
    highlights
        .runs(syntax.tree().unwrap(), text, 0..text.len(), &keys)
        .into_iter()
        .map(|(range, key)| (range.start, range.end, key))
        .collect()
}

#[test]
fn a_later_pattern_wins_and_an_inner_node_shows_on_top() {
    let query = "(pair) @property\n(string) @string\n(pair key: (string) @key)";
    let runs = json_runs(query, r#"{"a": 1}"#);
    assert_eq!(
        runs,
        [(1, 4, "key".to_owned()), (4, 7, "property".to_owned()),]
    );
}

#[test]
fn a_name_the_theme_lacks_hides_nothing_and_a_dropped_one_neither() {
    let query = "(pair) @property\n(string) @unknown\n(number) @dropped";
    assert_eq!(
        json_runs(query, r#"{"a": 1}"#),
        [(1, 7, "property".to_owned())]
    );
    assert_eq!(
        json_runs("(number) @renamed", "[1, 2]"),
        [(1, 2, "number".to_owned()), (4, 5, "number".to_owned())]
    );
}

#[test]
fn a_none_capture_hides_what_is_under_it() {
    let query = "(pair) @property\n(string) @none";
    assert_eq!(
        json_runs(query, r#"{"a": 1}"#),
        [(4, 7, "property".to_owned())]
    );
}

#[test]
fn a_first_wins_file_keeps_its_first_capture_of_a_node() {
    let lang: &'static Lang = Box::leak(Box::new(Lang {
        name: "test",
        suffixes: &[],
        grammar: || tree_sitter_json::LANGUAGE.into(),
        queries: Box::leak(Box::new([
            crate::git::languages::QueryFile {
                text: "(number) @number\n(number) @constant",
                first_wins: true,
            },
            crate::git::languages::QueryFile {
                text: "(string) @string\n(string) @property",
                first_wins: false,
            },
        ])),
        renames: &[],
    }));
    let highlights = compile(lang).unwrap();
    let keys = highlights.theme_keys(|_| true);
    let text = r#"["a", 1]"#;
    let mut syntax = Syntax::new(lang).unwrap();
    assert!(syntax.parse(text, Duration::from_secs(60)));
    let runs = highlights.runs(syntax.tree().unwrap(), text, 0..text.len(), &keys);
    let keys: Vec<&str> = runs.iter().map(|(_, key)| key.as_str()).collect();
    assert_eq!(keys, ["property", "number"]);
}

#[test]
fn a_slow_parse_moves_to_a_job_and_an_edit_drops_a_late_one() {
    let lang = lang_for("big.json").unwrap();
    let text: Arc<str> = format!("[{}0]", "1, ".repeat(400_000)).into();
    let mut syntax = Syntax::new(lang).unwrap();
    assert_eq!(syntax.lang().name, "JSON");
    assert!(!syntax.parse(&text, PARSE_BUDGET));
    assert!(!syntax.is_done());
    let job = syntax.job(text.clone());
    assert!(syntax.finish(job.run()));
    assert!(syntax.is_done());
    assert_eq!(syntax.tree().unwrap().root_node().end_byte(), text.len());

    // An edit after the job started: its tree is of an older text.
    let job = syntax.job(text.clone());
    let edited: Arc<str> = format!("{text} ").into();
    syntax.edit(&InputEdit {
        start_byte: text.len(),
        old_end_byte: text.len(),
        new_end_byte: edited.len(),
        start_position: Point::new(0, text.len()),
        old_end_position: Point::new(0, text.len()),
        new_end_position: Point::new(0, edited.len()),
    });
    assert!(!syntax.finish(job.run()));
    assert!(!syntax.is_done());
    assert!(syntax.parse(&edited, Duration::from_secs(60)));
    assert_eq!(syntax.tree().unwrap().root_node().end_byte(), edited.len());
}
