//! Reading the fields of a hook body as zod does: the same issues, in the order of the shape.

use natamux_core::zod::{self, Issues, Length, Rule};
use serde_json::{Map, Value, json};

/// `z.string().optional()`: None when the key is absent or wrong (the issue is pushed).
pub(crate) fn text(map: &Map<String, Value>, key: &str, issues: &mut Issues) -> Option<String> {
    let value = map.get(key)?;
    zod::string(Some(value), &[json!(key)], &[], issues)
}

/// `z.string().min(1)`.
pub(crate) fn event_name(map: &Map<String, Value>, key: &str, issues: &mut Issues) -> String {
    zod::string(map.get(key), &[json!(key)], &[Rule::Min(1)], issues).unwrap_or_default()
}

/// `z.record(z.string(), z.unknown()).optional()`.
pub(crate) fn record(
    map: &Map<String, Value>,
    key: &str,
    issues: &mut Issues,
) -> Option<Map<String, Value>> {
    match map.get(key)? {
        Value::Object(inner) => Some(inner.clone()),
        other => {
            issues.push(zod::invalid_type("record", Some(other), &[json!(key)]));
            None
        }
    }
}

/// `z.boolean().optional()`.
pub(crate) fn flag(map: &Map<String, Value>, key: &str, issues: &mut Issues) -> Option<bool> {
    match map.get(key)? {
        Value::Bool(value) => Some(*value),
        other => {
            issues.push(zod::invalid_type("boolean", Some(other), &[json!(key)]));
            None
        }
    }
}

/// `z.array(z.string()).optional()`.
pub(crate) fn strings(
    map: &Map<String, Value>,
    key: &str,
    issues: &mut Issues,
) -> Option<Vec<String>> {
    let value = map.get(key)?;
    zod::array(
        Some(value),
        &[json!(key)],
        Length::default(),
        issues,
        |item, path, issues| zod::string(Some(item), path, &[], issues),
    )
}

/// The object of a `z.unknown()` value, for a tool input. Anything else is an empty map.
pub(crate) fn object_or_empty(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}
