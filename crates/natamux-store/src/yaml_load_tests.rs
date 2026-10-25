use serde_json::json;

use super::*;

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    &value[name]
}

#[test]
fn copies_a_node_that_an_alias_points_to() {
    let value = load("base: &a {x: 1, y: [2, 3]}\ncopy: *a\nlist: [*a, *a]").unwrap();
    let base = json!({"x": 1, "y": [2, 3]});
    assert_eq!(
        value,
        json!({"base": base, "copy": base, "list": [base, base]})
    );
}

#[test]
fn fails_on_an_alias_without_an_anchor() {
    assert!(load("a: *missing").is_err());
}

#[test]
fn applies_a_merge_key_and_lets_the_own_key_win() {
    let value = load("base: &b {x: 1, y: 2}\nitem:\n  <<: *b\n  y: 3\n  z: 4").unwrap();
    assert_eq!(field(&value, "item"), &json!({"x": 1, "y": 3, "z": 4}));
}

#[test]
fn applies_a_merge_key_given_after_the_own_keys() {
    let value = load("base: &b {x: 1, y: 2}\nitem:\n  y: 3\n  <<: *b").unwrap();
    assert_eq!(field(&value, "item"), &json!({"x": 1, "y": 3}));
}

#[test]
fn merges_a_list_of_mappings_and_the_first_one_wins() {
    let value = load("a: &a {x: 1}\nb: &b {x: 2, y: 2}\nitem:\n  <<: [*a, *b]").unwrap();
    assert_eq!(field(&value, "item"), &json!({"x": 1, "y": 2}));
}

#[test]
fn fails_on_a_merge_of_something_that_is_not_a_mapping() {
    assert!(load("item:\n  <<: 5").is_err());
    assert!(load("item:\n  <<: [1, 2]").is_err());
}

#[test]
fn reads_a_double_less_than_sign_as_text_when_it_is_not_a_key() {
    let value = load("a: <<\nb: [<<]").unwrap();
    assert_eq!(value, json!({"a": "<<", "b": ["<<"]}));
}

#[test]
fn fails_on_a_key_given_twice() {
    let error = load("a: 1\nb: 2\na: 3").unwrap_err();
    assert!(error.contains("duplicated mapping key"), "{error}");
    assert!(load("outer:\n  a: 1\n  a: 2").is_err());
}

#[test]
fn reads_a_str_tag_as_text() {
    let value = load("a: !!str 12\nb: !!str true\nc: !!str null\nd: !!str").unwrap();
    assert_eq!(value, json!({"a": "12", "b": "true", "c": "null", "d": ""}));
}

#[test]
fn fails_on_a_tag_that_does_not_fit_the_value() {
    assert!(load("a: !!int abc").is_err());
    assert!(load("a: !!bool maybe").is_err());
    assert!(load("a: !!nothing x").is_err());
}

#[test]
fn reads_other_tags_that_fit_the_value() {
    let value = load("a: !!int 12\nb: !!float 1.5\nc: !!bool true\nd: !!null ~").unwrap();
    assert_eq!(value, json!({"a": 12, "b": 1.5, "c": true, "d": null}));
}

#[test]
fn marks_a_timestamp_as_a_date_at_any_depth() {
    let text = "at: 2026-10-08T09:00:00Z\n\
                day: 2026-10-08\n\
                offset: 2001-12-14t21:59:43.10-05:00\n\
                nested:\n  when: 2026-10-08T09:00:00Z\n\
                list:\n  - 2026-10-08T09:00:00Z\n\
                text: '2026-10-08T09:00:00Z'";
    let value = load(text).unwrap();
    assert_eq!(
        date_of(field(&value, "at")),
        Some("2026-10-08T09:00:00.000Z")
    );
    assert_eq!(
        date_of(field(&value, "day")),
        Some("2026-10-08T00:00:00.000Z")
    );
    assert_eq!(
        date_of(field(&value, "offset")),
        Some("2001-12-15T02:59:43.100Z")
    );
    assert_eq!(
        date_of(&value["nested"]["when"]),
        Some("2026-10-08T09:00:00.000Z")
    );
    assert_eq!(date_of(&value["list"][0]), Some("2026-10-08T09:00:00.000Z"));
    // A quoted timestamp is text.
    assert_eq!(field(&value, "text"), &json!("2026-10-08T09:00:00Z"));
    assert_eq!(date_of(field(&value, "text")), None);
}

#[test]
fn marks_a_document_that_is_a_timestamp_as_a_date() {
    let value = load("2026-10-08T09:00:00Z").unwrap();
    assert_eq!(date_of(&value), Some("2026-10-08T09:00:00.000Z"));
}

#[test]
fn does_not_take_a_map_with_other_keys_for_a_date() {
    assert_eq!(date_of(&json!({"a": 1})), None);
    assert_eq!(date_of(&json!("2026-10-08")), None);
    assert_eq!(date_of(&json!({"\u{0}date": "x", "b": 1})), None);
}

#[test]
fn gives_a_document_that_is_not_a_map_as_it_is() {
    assert_eq!(load("- a\n- 2\n- null").unwrap(), json!(["a", 2, null]));
    assert_eq!(load("just text").unwrap(), json!("just text"));
    assert_eq!(load("42").unwrap(), json!(42));
}

#[test]
fn gives_null_for_no_document() {
    assert_eq!(load("").unwrap(), Value::Null);
    assert_eq!(load("# only a comment\n").unwrap(), Value::Null);
}

#[test]
fn reads_only_the_first_document() {
    assert_eq!(load("a: 1\n---\nb: 2\n").unwrap(), json!({"a": 1}));
}

#[test]
fn reads_keys_that_are_not_text_as_js_does() {
    let value = load("1: a\ntrue: b\n~: c").unwrap();
    assert_eq!(value, json!({"1": "a", "true": "b", "null": "c"}));
}

#[test]
fn fails_on_yaml_that_does_not_parse() {
    assert!(load("a: [1").is_err());
    assert!(load("a: 'open").is_err());
}
