use serde_json::json;

use super::*;

fn data_of(value: Value) -> Value {
    value
}

#[test]
fn leaves_out_a_byte_order_mark() {
    let (data, content) = parse("\u{feff}---\na: 1\n---\nbody").unwrap();
    assert_eq!(data, data_of(json!({"a": 1})));
    assert_eq!(content, "body");
}

#[test]
fn reads_four_dashes_as_text_not_as_front_matter() {
    let text = "----\na: 1\n---\nbody";
    assert_eq!(parse(text).unwrap(), (json!({}), text.to_string()));
}

#[test]
fn reads_a_text_without_front_matter_as_content() {
    let text = "# Title\n\n---\nnot matter\n---\n";
    assert_eq!(parse(text).unwrap(), (json!({}), text.to_string()));
    assert_eq!(parse("").unwrap(), (json!({}), String::new()));
}

#[test]
fn reads_lines_that_end_with_crlf() {
    let (data, content) = parse("---\r\na: 1\r\nb: two\r\n---\r\nbody\r\n").unwrap();
    assert_eq!(data, data_of(json!({"a": 1, "b": "two"})));
    // Only the line break after the closing line is taken off, as gray-matter does.
    assert_eq!(content, "body\r\n");
}

#[test]
fn takes_a_language_after_the_first_delimiter() {
    for opening in ["---yaml", "--- yaml ", "---YAML", "---yml"] {
        let (data, content) = parse(&format!("{opening}\na: 1\n---\nx")).unwrap();
        assert_eq!(data, data_of(json!({"a": 1})), "{opening}");
        assert_eq!(content, "x", "{opening}");
    }
}

#[test]
fn refuses_a_language_it_has_no_engine_for() {
    let error = parse("---toml\na = 1\n---\nx").unwrap_err();
    assert_eq!(error, "gray-matter engine \"toml\" is not registered");
}

#[test]
fn gives_no_data_for_only_comments_or_nothing() {
    let (data, content) = parse("---\n# just a comment\n  # another\n---\nbody").unwrap();
    assert_eq!(data, json!({}));
    assert_eq!(content, "body");
    assert_eq!(
        parse("---\n---\nbody").unwrap(),
        (json!({}), "body".to_string())
    );
    assert_eq!(
        parse("---\n\n \n---\nbody").unwrap(),
        (json!({}), "body".to_string())
    );
}

#[test]
fn reads_a_line_with_only_a_hash_as_yaml_not_as_a_comment() {
    // `/^\s*#[^\n]+/` needs a character after the hash, so `#` alone is not taken out. YAML
    // then loads it as a comment, which is no document: null, as js-yaml gives.
    let (data, _) = parse("---\n#\n---\n").unwrap();
    assert_eq!(data, Value::Null);
}

#[test]
fn takes_the_rest_as_matter_when_there_is_no_closing_line() {
    let (data, content) = parse("---\na: 1\nb: 2\n").unwrap();
    assert_eq!(data, data_of(json!({"a": 1, "b": 2})));
    assert_eq!(content, "");
}

#[test]
fn reads_just_the_opening_delimiter_as_empty() {
    assert_eq!(parse("---").unwrap(), (json!({}), String::new()));
}

#[test]
fn takes_one_line_break_after_the_closing_line_and_keeps_the_rest() {
    let (_, content) = parse("---\na: 1\n---\n\nbody\n---\nmore\n").unwrap();
    assert_eq!(content, "\nbody\n---\nmore\n");
}

#[test]
fn ends_the_matter_at_the_first_line_break_and_three_dashes() {
    // The closing line is not matched by line: text after the dashes stays in the content.
    let (data, content) = parse("---\na: 1\n--- trailing\nx").unwrap();
    assert_eq!(data, data_of(json!({"a": 1})));
    assert_eq!(content, " trailing\nx");
}

#[test]
fn fails_on_yaml_that_does_not_load() {
    assert!(parse("---\na: [1\n---\n").is_err());
    assert!(parse("---\na: 1\na: 2\n---\n").is_err());
}

#[test]
fn gives_front_matter_that_is_not_a_mapping_as_it_is() {
    // gray-matter does not check what the YAML holds; the task reader refuses it later.
    let (data, _) = parse("---\n- a\n- b\n---\n").unwrap();
    assert_eq!(data, json!(["a", "b"]));
}

#[test]
fn writes_the_data_between_two_delimiters_and_then_the_content() {
    assert_eq!(
        stringify("Body", &json!({"a": 1})),
        "---\na: 1\n---\nBody\n"
    );
    assert_eq!(stringify("Body\n", &json!({})), "Body\n");
}

#[test]
fn reads_what_it_wrote() {
    let data = json!({"id": "T-0001", "title": "Fix: the bug", "labels": ["a", "b"]});
    let (read, content) = parse(&stringify("Notes\n", &data)).unwrap();
    assert_eq!(read, data);
    assert_eq!(content, "Notes\n");
}
