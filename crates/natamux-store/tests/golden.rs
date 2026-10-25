//! The task file fixtures: gray-matter and js-yaml wrote the expected files in CI
//! (`scripts/task-file-golden.mjs`), and the Rust reader and writer must give the same.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

use natamux_store::front_matter::{parse, stringify};
use natamux_store::yaml_load::date_of;
use serde_json::{Map, Value, json};

fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/front_matter")
}

fn names(suffix: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder())
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(suffix).map(str::to_string))
        .collect();
    names.sort();
    names
}

#[test]
fn writes_each_fixture_as_gray_matter_does() {
    let mut wrong = Vec::new();
    for name in names(".data.json") {
        let input: Value = serde_json::from_str(
            &fs::read_to_string(folder().join(format!("{name}.data.json"))).unwrap(),
        )
        .unwrap();
        let written = stringify(input["content"].as_str().unwrap(), &input["data"]);
        let expected = fs::read_to_string(folder().join(format!("{name}.md"))).unwrap_or_default();
        if written != expected {
            wrong.push(format!("{name}:\n--- rust\n{written}--- node\n{expected}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A date as the script writes it.
fn with_dates(value: Value) -> Value {
    if let Some(iso) = date_of(&value) {
        return json!({ "$date": iso });
    }
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(with_dates).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, each)| (key, with_dates(each)))
                .collect::<Map<_, _>>(),
        ),
        other => other,
    }
}

#[test]
fn reads_each_fixture_as_gray_matter_does() {
    let mut wrong = Vec::new();
    for name in names(".in.md") {
        let text = fs::read_to_string(folder().join(format!("{name}.in.md"))).unwrap();
        let read = match parse(&text) {
            Ok((data, content)) => json!({ "data": with_dates(data), "content": content }),
            Err(_) => json!({ "error": true }),
        };
        let expected: Value = fs::read_to_string(folder().join(format!("{name}.read.json")))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        if read != expected {
            wrong.push(format!("{name}: rust {read} node {expected}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
