use serde_json::json;

use super::*;

#[test]
fn writes_a_task_shape_as_js_yaml() {
    let value = json!({
        "id": "T-0001",
        "title": "Fix: the bug",
        "state": "draft",
        "labels": [],
        "agent": null,
        "stages": ["spec", {"id": "build", "parallel": ["a", "b"]}],
        "lanes": {},
        "context": {"source": "manual", "ref": ""},
        "withoutAgent": true,
        "createdAt": "2026-10-08T09:00:00.000Z",
    });
    assert_eq!(
        dump(&value),
        "id: T-0001\n\
         title: 'Fix: the bug'\n\
         state: draft\n\
         labels: []\n\
         agent: null\n\
         stages:\n  - spec\n  - id: build\n    parallel:\n      - a\n      - b\n\
         lanes: {}\n\
         context:\n  source: manual\n  ref: ''\n\
         withoutAgent: true\n\
         createdAt: '2026-10-08T09:00:00.000Z'\n"
    );
}

#[test]
fn quotes_what_another_type_or_yaml_1_1_would_read() {
    for (text, written) in [
        ("yes", "'yes'"),
        ("No", "'No'"),
        ("null", "'null'"),
        ("~", "'~'"),
        ("123", "'123'"),
        ("1.5", "'1.5'"),
        ("0x1F", "'0x1F'"),
        ("2026-10-08", "'2026-10-08'"),
        ("<<", "'<<'"),
        ("true", "'true'"),
        ("- item", "'- item'"),
        ("a # b", "'a # b'"),
        ("a#b", "a#b"),
        (" lead", "' lead'"),
        ("it's", "it's"),
        ("'quoted'", "'''quoted'''"),
        ("Tiếng Việt", "Tiếng Việt"),
        ("😀 smile", "\"\\U0001F600 smile\""),
        ("bell\u{7}", "\"bell\\a\""),
        ("bom\u{feff}", "\"bom\\uFEFF\""),
    ] {
        assert_eq!(
            dump(&json!({ "k": text })),
            format!("k: {written}\n"),
            "{text:?}"
        );
    }
}

#[test]
fn writes_line_breaks_as_a_literal_block() {
    assert_eq!(dump(&json!({ "k": "one\ntwo" })), "k: |-\n  one\n  two\n");
    assert_eq!(dump(&json!({ "k": "one\n" })), "k: |\n  one\n");
    assert_eq!(dump(&json!({ "k": "one\n\n" })), "k: |+\n  one\n\n");
    assert_eq!(
        dump(&json!({ "k": " indented\nnext" })),
        "k: |2-\n   indented\n  next\n"
    );
}

#[test]
fn folds_a_line_longer_than_the_width() {
    let long = ["word"; 20].join(" ");
    let written = dump(&json!({ "k": long }));
    assert_eq!(
        written,
        "k: >-\n  word word word word word word word word word word word word word word word\n  word word word word word\n"
    );
}

#[test]
fn writes_numbers_as_js_yaml() {
    assert_eq!(
        dump(&json!({"a": 1, "b": 1.5, "c": -0.0, "d": 1e21})),
        "a: 1\nb: 1.5\nc: -0.0\nd: 1e+21\n"
    );
}
