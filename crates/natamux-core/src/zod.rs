//! The parts of zod 4.6.7 the task routes use: the same issues, in the same order, with the same
//! fields and texts, so a 400 answer is the Node daemon's byte for byte.

use serde_json::{Map, Value, json};

use crate::js::{js_trim, utf16_len};

/// Where an issue is: object keys and array indexes.
pub type Path = Vec<Value>;

pub fn at(path: &[Value], key: impl Into<Value>) -> Path {
    let mut next = path.to_vec();
    next.push(key.into());
    next
}

/// One zod issue: its own fields in zod's order, then `path` and `message`.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue(pub Map<String, Value>);

impl Issue {
    pub fn new(fields: Vec<(&str, Value)>, path: &[Value], message: String) -> Issue {
        let mut map: Map<String, Value> = fields
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect();
        map.insert("path".into(), Value::Array(path.to_vec()));
        map.insert("message".into(), Value::String(message));
        Issue(map)
    }

    pub fn custom(path: &[Value], message: &str) -> Issue {
        Issue::new(vec![("code", json!("custom"))], path, message.into())
    }

    pub fn code(&self) -> &str {
        self.0.get("code").and_then(Value::as_str).unwrap_or("")
    }

    pub fn message(&self) -> &str {
        self.0.get("message").and_then(Value::as_str).unwrap_or("")
    }

    pub fn path(&self) -> &[Value] {
        self.0
            .get("path")
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }

    /// zod keeps going after these; a type error or a bad option stops the checks of the schema
    /// that holds it (`util.aborted`).
    fn continues(&self) -> bool {
        matches!(
            self.code(),
            "too_big" | "too_small" | "invalid_format" | "unrecognized_keys" | "custom"
        )
    }
}

/// A failed parse: its issues, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct ZodError(pub Vec<Issue>);

impl ZodError {
    /// The issues as the answer carries them.
    pub fn issues(&self) -> Value {
        Value::Array(
            self.0
                .iter()
                .map(|issue| Value::Object(issue.0.clone()))
                .collect(),
        )
    }

    /// One line a client can show (`describeIssues` in `errors.ts`).
    pub fn describe(&self) -> String {
        self.0
            .iter()
            .map(|issue| {
                let path: Vec<String> = issue.path().iter().map(path_part).collect();
                if path.is_empty() {
                    issue.message().to_string()
                } else {
                    format!("{}: {}", path.join("."), issue.message())
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// `error.message` of a ZodError, as `String(error)` logs it.
    pub fn to_json_text(&self) -> String {
        crate::json::to_js_string(&self.issues())
    }
}

impl std::fmt::Display for ZodError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.describe())
    }
}

impl std::error::Error for ZodError {}

fn path_part(part: &Value) -> String {
    match part {
        Value::String(text) => text.clone(),
        other => crate::json::to_js_string(other),
    }
}

/// The issues of one parse, and whether any of them stops further checks.
#[derive(Default)]
pub struct Issues(pub Vec<Issue>);

impl Issues {
    pub fn push(&mut self, issue: Issue) {
        self.0.push(issue);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `util.aborted(payload, from)`.
    pub fn aborted_since(&self, from: usize) -> bool {
        self.0[from..].iter().any(|issue| !issue.continues())
    }

    pub fn into_result<T>(self, value: Option<T>) -> Result<T, ZodError> {
        match value {
            Some(value) if self.0.is_empty() => Ok(value),
            _ => Err(ZodError(self.0)),
        }
    }
}

/// `util.parsedType` of a JSON value; None is `undefined`.
pub fn type_name(value: Option<&Value>) -> &'static str {
    match value {
        None => "undefined",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

pub fn invalid_type(expected: &str, input: Option<&Value>, path: &[Value]) -> Issue {
    Issue::new(
        vec![
            ("expected", json!(expected)),
            ("code", json!("invalid_type")),
        ],
        path,
        format!(
            "Invalid input: expected {expected}, received {}",
            type_name(input)
        ),
    )
}

pub fn too_big(origin: &str, maximum: usize, path: &[Value]) -> Issue {
    let unit = if origin == "array" {
        "items"
    } else {
        "characters"
    };
    Issue::new(
        vec![
            ("origin", json!(origin)),
            ("code", json!("too_big")),
            ("maximum", json!(maximum)),
            ("inclusive", json!(true)),
        ],
        path,
        format!("Too big: expected {origin} to have <={maximum} {unit}"),
    )
}

pub fn too_small(origin: &str, minimum: usize, path: &[Value]) -> Issue {
    let unit = if origin == "array" {
        "items"
    } else {
        "characters"
    };
    Issue::new(
        vec![
            ("origin", json!(origin)),
            ("code", json!("too_small")),
            ("minimum", json!(minimum)),
            ("inclusive", json!(true)),
        ],
        path,
        format!("Too small: expected {origin} to have >={minimum} {unit}"),
    )
}

/// `z.enum(values)` or `z.literal(value)` refused the input.
pub fn invalid_value(values: Value, path: &[Value]) -> Issue {
    let list: Vec<String> = values
        .as_array()
        .map(|values| values.iter().map(primitive_text).collect())
        .unwrap_or_default();
    let message = if list.len() == 1 {
        format!("Invalid input: expected {}", list[0])
    } else {
        format!("Invalid option: expected one of {}", list.join("|"))
    };
    Issue::new(
        vec![("code", json!("invalid_value")), ("values", values)],
        path,
        message,
    )
}

/// `util.stringifyPrimitive`: strings in quotes, with nothing escaped.
fn primitive_text(value: &Value) -> String {
    match value {
        Value::String(text) => format!("\"{text}\""),
        other => crate::json::to_js_string(other),
    }
}

pub fn unrecognized_keys(keys: Vec<String>, path: &[Value]) -> Issue {
    let plural = if keys.len() > 1 { "s" } else { "" };
    let list: Vec<String> = keys.iter().map(|key| format!("\"{key}\"")).collect();
    let message = format!("Unrecognized key{plural}: {}", list.join(", "));
    Issue::new(
        vec![("code", json!("unrecognized_keys")), ("keys", json!(keys))],
        path,
        message,
    )
}

/// A regex check: the test, the regex as JS writes it, and the schema's own message.
#[derive(Clone, Copy)]
pub struct Pattern {
    pub test: fn(&str) -> bool,
    pub source: &'static str,
    pub message: Option<&'static str>,
}

/// One check of a `z.string()` chain, in the order the schema lists them.
#[derive(Clone, Copy)]
pub enum Rule {
    Trim,
    Min(usize),
    Max(usize),
    Regex(Pattern),
    Datetime,
    /// `z.url({ protocol: /^https?$/ })`.
    HttpUrl,
}

/// `z.string()` with its checks. None: the value failed (its issues are pushed).
pub fn string(
    value: Option<&Value>,
    path: &[Value],
    rules: &[Rule],
    issues: &mut Issues,
) -> Option<String> {
    let Some(Value::String(text)) = value else {
        issues.push(invalid_type("string", value, path));
        return None;
    };
    let before = issues.len();
    let mut text = text.clone();
    for rule in rules {
        match rule {
            Rule::Trim => text = js_trim(&text).to_string(),
            Rule::Min(min) if utf16_len(&text) < *min => {
                issues.push(too_small("string", *min, path));
            }
            Rule::Max(max) if utf16_len(&text) > *max => {
                issues.push(too_big("string", *max, path));
            }
            Rule::Regex(pattern) if !(pattern.test)(&text) => {
                let message = pattern.message.map_or_else(
                    || format!("Invalid string: must match pattern {}", pattern.source),
                    str::to_string,
                );
                issues.push(Issue::new(
                    vec![
                        ("origin", json!("string")),
                        ("code", json!("invalid_format")),
                        ("format", json!("regex")),
                        ("pattern", json!(pattern.source)),
                    ],
                    path,
                    message,
                ));
            }
            Rule::Datetime if !crate::formats::is_iso_datetime(&text) => {
                issues.push(Issue::new(
                    vec![
                        ("origin", json!("string")),
                        ("code", json!("invalid_format")),
                        ("format", json!("datetime")),
                        ("pattern", json!(crate::formats::DATETIME_SOURCE)),
                    ],
                    path,
                    "Invalid ISO datetime".into(),
                ));
            }
            Rule::HttpUrl => match crate::formats::check_http_url(&text) {
                Ok(kept) => text = kept,
                Err(note) => {
                    let mut fields =
                        vec![("code", json!("invalid_format")), ("format", json!("url"))];
                    if let Some(note) = note {
                        fields.push(("note", json!(note)));
                    }
                    issues.push(Issue::new(fields, path, "Invalid URL".into()));
                }
            },
            _ => {}
        }
    }
    (issues.len() == before).then_some(text)
}

/// `z.enum(values)`.
pub fn one_of(
    value: Option<&Value>,
    path: &[Value],
    values: &[&str],
    issues: &mut Issues,
) -> Option<String> {
    match value {
        Some(Value::String(text)) if values.contains(&text.as_str()) => Some(text.clone()),
        _ => {
            issues.push(invalid_value(json!(values), path));
            None
        }
    }
}

/// The length checks of `z.array(item).min(min).max(max)`.
#[derive(Clone, Copy, Default)]
pub struct Length {
    pub min: Option<usize>,
    pub max: Option<usize>,
}

/// `z.array(item)` with its length checks. Element issues come first, then the length checks.
pub fn array<T>(
    value: Option<&Value>,
    path: &[Value],
    length: Length,
    issues: &mut Issues,
    mut item: impl FnMut(&Value, &[Value], &mut Issues) -> Option<T>,
) -> Option<Vec<T>> {
    let Some(Value::Array(items)) = value else {
        issues.push(invalid_type("array", value, path));
        return None;
    };
    let before = issues.len();
    let mut out = Vec::new();
    for (index, each) in items.iter().enumerate() {
        if let Some(parsed) = item(each, &at(path, index), issues) {
            out.push(parsed);
        }
    }
    // Length checks have a `when` in zod, so an element's error does not skip them.
    if let Some(min) = length.min.filter(|min| items.len() < *min) {
        issues.push(too_small("array", min, path));
    }
    if let Some(max) = length.max.filter(|max| items.len() > *max) {
        issues.push(too_big("array", max, path));
    }
    (issues.len() == before).then_some(out)
}

/// Runs a parse on its own issues: `.catch(...)` and union options throw theirs away.
pub fn quietly<T>(parse: impl FnOnce(&mut Issues) -> Option<T>) -> Option<T> {
    let mut issues = Issues::default();
    let value = parse(&mut issues);
    if issues.is_empty() { value } else { None }
}

/// `z.object(shape)`: the object, or an issue when the value is none.
pub fn object<'a>(
    value: Option<&'a Value>,
    path: &[Value],
    issues: &mut Issues,
) -> Option<&'a Map<String, Value>> {
    match value {
        Some(Value::Object(map)) => Some(map),
        _ => {
            issues.push(invalid_type("object", value, path));
            None
        }
    }
}

/// `.strict()`: the keys the shape does not have, in JS order.
pub fn strict(map: &Map<String, Value>, shape: &[&str], path: &[Value], issues: &mut Issues) {
    let unknown: Vec<String> = crate::json::js_keys(map)
        .into_iter()
        .filter(|key| !shape.contains(&key.as_str()))
        .cloned()
        .collect();
    if !unknown.is_empty() {
        issues.push(unrecognized_keys(unknown, path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_texts_of_zod() {
        let mut issues = Issues::default();
        let title = string(
            Some(&json!(" x".repeat(201))),
            &[json!("title")],
            &[Rule::Trim, Rule::Min(1), Rule::Max(400)],
            &mut issues,
        );
        assert_eq!(title, None);
        let error = ZodError(issues.0);
        assert_eq!(
            error.describe(),
            "title: Too big: expected string to have <=400 characters"
        );
        assert_eq!(
            crate::json::to_js_string(&error.issues()),
            r#"[{"origin":"string","code":"too_big","maximum":400,"inclusive":true,"path":["title"],"message":"Too big: expected string to have <=400 characters"}]"#
        );
        assert_eq!(
            invalid_value(json!(["low", "high"]), &[]).message(),
            "Invalid option: expected one of \"low\"|\"high\""
        );
        assert_eq!(
            invalid_value(json!([true]), &[]).message(),
            "Invalid input: expected true"
        );
        assert_eq!(
            invalid_type("object", None, &[]).message(),
            "Invalid input: expected object, received undefined"
        );
        assert_eq!(
            unrecognized_keys(vec!["a".into(), "b".into()], &[]).message(),
            "Unrecognized keys: \"a\", \"b\""
        );
    }
}
