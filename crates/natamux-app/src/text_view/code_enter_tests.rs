#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::git::languages::lang_for;
use crate::text_view::code_langs::typing_for;

/// The text after Enter at the `|` of `marked` in a file named `path`, with the caret as `|`.
fn after_enter(path: &str, marked: &str) -> String {
    let at = marked.find('|').unwrap();
    let text = marked.replace('|', "");
    let lang = lang_for(path).unwrap();
    let typing = typing_for(lang.name).unwrap();
    let tree = parse(lang, &text);
    let edit = enter(typing, lang, &text, at..at, tree.as_ref());
    let mut out = text.clone();
    out.replace_range(edit.range, &edit.text);
    out.insert(edit.caret, '|');
    out
}

#[test]
fn a_list_item_goes_on_with_the_next_marker() {
    assert_eq!(after_enter("a.md", "- item|"), "- item\n- |");
    assert_eq!(after_enter("a.md", "* item|"), "* item\n* |");
    assert_eq!(after_enter("a.md", "9. item|"), "9. item\n10. |");
    assert_eq!(after_enter("a.md", "- [x] done|"), "- [x] done\n- [ ] |");
    assert_eq!(after_enter("a.md", "  - nested|"), "  - nested\n  - |");
}

#[test]
fn an_empty_list_item_goes_back_a_level_or_loses_its_marker() {
    assert_eq!(after_enter("a.md", "- a\n- |"), "- a\n|");
    assert_eq!(after_enter("a.md", "1. a\n2. |"), "1. a\n|");
    assert_eq!(after_enter("a.md", "  - a\n  - |"), "  - a\n- |");
}

#[test]
fn a_quote_is_not_a_list() {
    assert_eq!(after_enter("a.md", "> quote|"), "> quote\n|");
}

#[test]
fn a_line_comment_goes_on() {
    assert_eq!(after_enter("a.rs", "    // ab|"), "    // ab\n    // |");
    assert_eq!(after_enter("a.py", "# ab|"), "# ab\n# |");
}

#[test]
fn a_doc_comment_goes_on_and_its_end_moves_down() {
    assert_eq!(after_enter("a.rs", "/*| */"), "/*\n * |\n */");
    assert_eq!(after_enter("a.ts", "/**| */"), "/**\n * |\n */");
    assert_eq!(
        after_enter("a.ts", "/**\n * a|\n */"),
        "/**\n * a\n * |\n */"
    );
    // A plain block comment of TypeScript does not go on.
    assert_eq!(after_enter("a.ts", "/*| */"), "/*\n|*/");
}

#[test]
fn the_spaces_after_the_caret_go() {
    assert_eq!(after_enter("a.css", "/* ab| */"), "/* ab\n|*/");
}

#[test]
fn a_block_pair_puts_the_caret_on_a_deeper_line_of_its_own() {
    assert_eq!(after_enter("a.json", "[|]"), "[\n    |\n]");
    assert_eq!(after_enter("a.go", "func f() {|}"), "func f() {\n\t|\n}");
    assert_eq!(after_enter("a.md", "(|)"), "(\n|\n)");
}

#[test]
fn a_line_after_a_colon_in_python_goes_one_level_deeper() {
    assert_eq!(after_enter("a.py", "def f():|"), "def f():\n    |");
}

#[test]
fn a_shell_or_yaml_line_that_opens_a_block_goes_one_level_deeper() {
    assert_eq!(after_enter("a.sh", "if a; then|"), "if a; then\n    |");
    assert_eq!(after_enter("a.sh", "echo a|"), "echo a\n|");
    assert_eq!(after_enter("a.yaml", "a:|"), "a:\n    |");
    assert_eq!(after_enter("a.yaml", "a: b|"), "a: b\n|");
}

#[test]
fn a_top_level_c_string_goes_on_one_level_deeper() {
    assert_eq!(after_enter("a.c", "\"|\""), "\"\n    |\"");
}
