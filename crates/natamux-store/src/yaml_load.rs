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

/// S-22: a few lines of nested aliases can copy a node billions of times. Far above any task file.
const MAX_NODES: usize = 100_000;

#[derive(Default)]
struct Composer {
    stack: Vec<Frame>,
    /// Each anchored value with its node count, so an alias is refused before it is copied.
    anchors: HashMap<usize, (Value, usize)>,
    nodes: usize,
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
            .and_then(|value| {
                self.add_nodes(1)?;
                self.node(value, anchor);
                Ok(())
            }),
            Event::Alias(id) => self.alias(id),
            Event::SequenceStart(anchor, _) => self.add_nodes(1).map(|()| {
                self.stack.push(Frame::Seq(Vec::new(), anchor));
            }),
            Event::MappingStart(anchor, _) => self.add_nodes(1).map(|()| {
                self.stack.push(Frame::Map {
                    map: Map::new(),
                    key: None,
                    anchor,
                    merged: Vec::new(),
                });
            }),
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

/// The nodes of a value, counted without recursion.
fn count_nodes(value: &Value) -> usize {
    let mut count = 0;
    let mut todo = vec![value];
    while let Some(next) = todo.pop() {
        count += 1;
        match next {
            Value::Array(items) => todo.extend(items),
            Value::Object(map) => todo.extend(map.values()),
            _ => {}
        }
    }
    count
}

impl Composer {
    fn add_nodes(&mut self, count: usize) -> Result<(), String> {
        self.nodes = self.nodes.saturating_add(count);
        if self.nodes > MAX_NODES {
            return Err(format!("the document has more than {MAX_NODES} nodes"));
        }
        Ok(())
    }

    /// The copy counts as its nodes, and is refused before it is made.
    fn alias(&mut self, id: usize) -> Result<(), String> {
        let size = self
            .anchors
            .get(&id)
            .map(|(_, size)| *size)
            .ok_or("unidentified alias")?;
        self.add_nodes(size)?;
        if let Some((value, _)) = self.anchors.get(&id) {
            let value = value.clone();
            self.node(value, 0);
        }
        Ok(())
    }

    /// A finished node goes into its parent, or is the document.
    fn node(&mut self, value: Value, anchor: usize) {
        if anchor > 0 {
            let size = count_nodes(&value);
            self.anchors.insert(anchor, (value.clone(), size));
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
