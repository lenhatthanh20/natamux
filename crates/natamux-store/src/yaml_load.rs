//! YAML read as js-yaml 3.15.2 `safeLoad` reads it: the parser gives the events, and this file
//! gives plain scalars js-yaml's meaning (`yaml_types`), keeps anchors, applies merge keys and
//! refuses a key given twice. A timestamp becomes a marked date (`date_value`), as js-yaml gives a
//! Date: only the task reader turns the dates at the top level into text.

use std::collections::HashMap;

use serde_json::{Map, Value};
use yaml_rust2::parser::{Event, EventReceiver, Parser};
use yaml_rust2::scanner::TScalarStyle;

use crate::yaml_types::{Plain, resolve_plain};

/// The key of the one-key object that stands for a date.
const DATE_KEY: &str = "\u{0}date";

pub fn date_value(iso: String) -> Value {
    let mut map = Map::new();
    map.insert(DATE_KEY.into(), Value::String(iso));
    Value::Object(map)
}

/// The ISO text of a date read from YAML.
pub fn date_of(value: &Value) -> Option<&str> {
    let map = value.as_object()?;
    if map.len() != 1 {
        return None;
    }
    map.get(DATE_KEY)?.as_str()
}

/// The first document; null for none.
pub fn load(text: &str) -> Result<Value, String> {
    let mut composer = Composer::default();
    let mut parser = Parser::new_from_str(text);
    parser
        .load(&mut composer, false)
        .map_err(|error| error.to_string())?;
    if let Some(error) = composer.error {
        return Err(error);
    }
    Ok(composer.document.unwrap_or(Value::Null))
}

enum Frame {
    Seq(Vec<Value>, usize),
    Map {
        map: Map<String, Value>,
        key: Option<Value>,
        anchor: usize,
        merged: Vec<String>,
    },
}

#[derive(Default)]
struct Composer {
    stack: Vec<Frame>,
    anchors: HashMap<usize, Value>,
    document: Option<Value>,
    error: Option<String>,
}

impl EventReceiver for Composer {
    fn on_event(&mut self, event: Event) {
        if self.error.is_some() {
            return;
        }
        let result = match event {
            Event::Scalar(text, style, anchor, tag) => scalar(
                text,
                style,
                tag.map(|tag| format!("{}{}", tag.handle, tag.suffix)),
            )
            .map(|value| self.node(value, anchor)),
            Event::Alias(id) => match self.anchors.get(&id).cloned() {
                Some(value) => {
                    self.node(value, 0);
                    Ok(())
                }
                None => Err("unidentified alias".into()),
            },
            Event::SequenceStart(anchor, _) => {
                self.stack.push(Frame::Seq(Vec::new(), anchor));
                Ok(())
            }
            Event::MappingStart(anchor, _) => {
                self.stack.push(Frame::Map {
                    map: Map::new(),
                    key: None,
                    anchor,
                    merged: Vec::new(),
                });
                Ok(())
            }
            Event::SequenceEnd => match self.stack.pop() {
                Some(Frame::Seq(items, anchor)) => {
                    self.node(Value::Array(items), anchor);
                    Ok(())
                }
                _ => Err("broken sequence".into()),
            },
            Event::MappingEnd => match self.stack.pop() {
                Some(Frame::Map { map, anchor, .. }) => {
                    self.node(Value::Object(map), anchor);
                    Ok(())
                }
                _ => Err("broken mapping".into()),
            },
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.error = Some(error);
        }
    }
}

/// A scalar, with its tag when it has one (`!!str`, `!!int`…).
fn scalar(text: String, style: TScalarStyle, tag: Option<String>) -> Result<Value, String> {
    let core = "tag:yaml.org,2002:";
    let tag = tag.map(|tag| tag.replace("!!", core));
    match tag.as_deref() {
        None if style == TScalarStyle::Plain => Ok(match resolve_plain(&text) {
            Plain::Value(value) => value,
            Plain::Timestamp(iso) => date_value(iso),
            Plain::Merge => Value::String(MERGE.into()),
        }),
        None | Some("!") => Ok(Value::String(text)),
        Some(tag) if tag == format!("{core}str") => Ok(Value::String(text)),
        Some(tag) if tag.starts_with(core) => {
            let kind = &tag[core.len()..];
            match (kind, resolve_plain(&text)) {
                ("null", Plain::Value(Value::Null))
                | ("bool", Plain::Value(Value::Bool(_)))
                | ("int" | "float", Plain::Value(Value::Number(_))) => {
                    Ok(match resolve_plain(&text) {
                        Plain::Value(value) => value,
                        _ => Value::Null,
                    })
                }
                ("timestamp", Plain::Timestamp(iso)) => Ok(date_value(iso)),
                _ => Err(format!("cannot resolve a node with !<{tag}> explicit tag")),
            }
        }
        Some(tag) => Err(format!("unknown tag !<{tag}>")),
    }
}

/// A plain `<<`: the merge key, not text.
const MERGE: &str = "\u{0}merge";

/// Anywhere but a key, `<<` is the text it is.
fn as_text(value: Value) -> Value {
    if value == Value::String(MERGE.into()) {
        Value::String("<<".into())
    } else {
        value
    }
}

impl Composer {
    /// A finished node goes into its parent, or is the document.
    fn node(&mut self, value: Value, anchor: usize) {
        if anchor > 0 {
            self.anchors.insert(anchor, value.clone());
        }
        match self.stack.last_mut() {
            None => {
                if self.document.is_none() {
                    self.document = Some(as_text(value));
                }
            }
            Some(Frame::Seq(items, _)) => items.push(as_text(value)),
            Some(Frame::Map {
                key: key @ None, ..
            }) => *key = Some(value),
            Some(Frame::Map {
                map, key, merged, ..
            }) => {
                let Some(key) = key.take() else {
                    return;
                };
                if let Err(error) = store_pair(map, merged, key, as_text(value)) {
                    self.error = Some(error);
                }
            }
        }
    }
}

/// `storeMappingPair`: merge keys copy what is not set; a key given twice is an error, unless the
/// first came from a merge.
fn store_pair(
    map: &mut Map<String, Value>,
    merged: &mut Vec<String>,
    key: Value,
    value: Value,
) -> Result<(), String> {
    if key == Value::String(MERGE.into()) {
        let sources = match value {
            Value::Array(items) => items,
            other => vec![other],
        };
        for source in sources {
            let Value::Object(source) = source else {
                return Err(
                    "cannot merge mappings; the provided source object is unacceptable".into(),
                );
            };
            for (name, each) in source {
                if !map.contains_key(&name) {
                    map.insert(name.clone(), each);
                    merged.push(name);
                }
            }
        }
        return Ok(());
    }
    let name = key_text(&key);
    if map.contains_key(&name) && !merged.contains(&name) {
        return Err("duplicated mapping key".into());
    }
    merged.retain(|each| *each != name);
    map.insert(name, value);
    Ok(())
}

/// `String(key)` of a key that is not text.
fn key_text(key: &Value) -> String {
    match key {
        Value::String(text) => text.clone(),
        Value::Null => "null".into(),
        Value::Array(items) => items.iter().map(key_text).collect::<Vec<_>>().join(","),
        Value::Object(_) => date_of(key).map_or_else(|| "[object Object]".into(), str::to_string),
        other => natamux_core::json::to_js_string(other),
    }
}

#[cfg(test)]
#[path = "yaml_load_tests.rs"]
mod tests;
