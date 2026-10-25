//! `taskSchema` and its parts (`packages/core/src/tasks/task.ts`): what a task file, an index row
//! or a new task must hold, with zod's defaults, fallbacks and texts.

use natamux_proto::projects::repo::CONTEXT_SOURCE_TYPES;
use natamux_proto::tasks::task::{
    CHECK_STATES, PULL_REQUEST_STATES, TASK_PRIORITIES, TASK_STATES, Task,
};
use serde_json::{Map, Value, json};

use crate::formats::{REPO_ID, STAGE_ID, TASK_ID};
use crate::tasks::task_parts::{lanes, step_refs, string_record};
use crate::zod::{
    Issue, Issues, Length, Rule, ZodError, array, at, object, one_of, quietly, string,
};

/// `LEGACY_STEP_REFS`: the steps of a task made before stages existed.
pub fn legacy_step_refs() -> Value {
    json!(["implement", "review", "release"])
}

pub fn validate_task(value: &Value) -> Result<Task, ZodError> {
    let mut issues = Issues::default();
    let fields = task_fields(value, &[], &mut issues);
    let fields = issues.into_result(fields)?;
    serde_json::from_value(Value::Object(fields))
        .map_err(|error| ZodError(vec![Issue::custom(&[], &error.to_string())]))
}

/// The task as zod's output: every key in the schema's order.
pub fn task_fields(
    value: &Value,
    path: &[Value],
    issues: &mut Issues,
) -> Option<Map<String, Value>> {
    let map = object(Some(value), path, issues)?;
    let before = issues.len();
    let mut out = Map::new();
    head_fields(map, path, issues, &mut out);
    stage_fields(map, path, issues, &mut out);
    tail_fields(map, path, issues, &mut out);
    (issues.len() == before).then_some(out)
}

/// The fields from `id` to `prStatus`.
fn head_fields(
    map: &Map<String, Value>,
    path: &[Value],
    issues: &mut Issues,
    out: &mut Map<String, Value>,
) {
    let get = |key: &str| map.get(key);
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            out.insert(key.into(), value);
        }
    };
    put(
        "id",
        string(get("id"), &at(path, "id"), &TASK_ID, issues).map(Value::from),
    );
    put("title", title(get("title"), &at(path, "title"), issues));
    put(
        "description",
        defaulted(get("description"), json!(""), |v| {
            string(Some(v), &at(path, "description"), &[], issues).map(Value::from)
        }),
    );
    put(
        "state",
        defaulted(get("state"), json!("draft"), |v| {
            one_of(Some(v), &at(path, "state"), &TASK_STATES, issues).map(Value::from)
        }),
    );
    put(
        "priority",
        defaulted(get("priority"), json!("normal"), |v| {
            one_of(Some(v), &at(path, "priority"), &TASK_PRIORITIES, issues).map(Value::from)
        }),
    );
    put(
        "labels",
        defaulted(get("labels"), json!([]), |v| {
            labels(v, &at(path, "labels"), &[Rule::Min(1)], issues)
        }),
    );
    for key in ["agent", "branch", "workspace"] {
        put(
            key,
            nullable(get(key), |v| {
                string(Some(v), &at(path, key), &[Rule::Min(1)], issues).map(Value::from)
            }),
        );
    }
    put("pr", Some(caught_url(get("pr"))));
    put(
        "prStatus",
        nullable(get("prStatus"), |v| {
            pull_request_status(v, &at(path, "prStatus"), issues)
        }),
    );
}

/// The fields from `dependsOn` to `backgroundSince`.
fn stage_fields(
    map: &Map<String, Value>,
    path: &[Value],
    issues: &mut Issues,
    out: &mut Map<String, Value>,
) {
    let get = |key: &str| map.get(key);
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            out.insert(key.into(), value);
        }
    };
    put(
        "dependsOn",
        defaulted(get("dependsOn"), json!([]), |v| {
            task_ids(v, &at(path, "dependsOn"), issues)
        }),
    );
    put(
        "parentId",
        nullable(get("parentId"), |v| {
            string(Some(v), &at(path, "parentId"), &TASK_ID, issues).map(Value::from)
        }),
    );
    put(
        "repo",
        nullable(get("repo"), |v| {
            string(Some(v), &at(path, "repo"), &REPO_ID, issues).map(Value::from)
        }),
    );
    put(
        "stage",
        defaulted(get("stage"), json!("implement"), |v| {
            string(Some(v), &at(path, "stage"), &STAGE_ID, issues).map(Value::from)
        }),
    );
    put(
        "stages",
        defaulted(get("stages"), legacy_step_refs(), |v| {
            step_refs(v, &at(path, "stages"), issues)
        }),
    );
    put(
        "lanes",
        defaulted(get("lanes"), json!({}), |v| {
            lanes(v, &at(path, "lanes"), issues)
        }),
    );
    put(
        "paths",
        defaulted(get("paths"), json!({}), |v| {
            string_record(v, &at(path, "paths"), issues)
        }),
    );
    for key in ["stageSince", "backgroundSince"] {
        put(
            key,
            nullable(get(key), |v| {
                string(Some(v), &at(path, key), &[Rule::Datetime], issues).map(Value::from)
            }),
        );
    }
}

/// The fields from `workflowStages` to `updatedAt`.
fn tail_fields(
    map: &Map<String, Value>,
    path: &[Value],
    issues: &mut Issues,
    out: &mut Map<String, Value>,
) {
    let get = |key: &str| map.get(key);
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            out.insert(key.into(), value);
        }
    };
    let stage_list = |v: Option<&Value>| {
        quietly(|quiet| {
            array(v, &[], Length::default(), quiet, |each, at_path, quiet| {
                string(Some(each), at_path, &STAGE_ID, quiet).map(Value::from)
            })
        })
        .map(Value::Array)
    };
    put(
        "workflowStages",
        Some(stage_list(get("workflowStages")).unwrap_or(json!([]))),
    );
    for key in ["ciStages", "skipStages"] {
        put(key, get(key).and_then(|v| stage_list(Some(v))));
    }
    put(
        "withoutAgent",
        get("withoutAgent").filter(|v| **v == json!(true)).cloned(),
    );
    put(
        "context",
        nullable(get("context"), |v| {
            task_context(v, &at(path, "context"), issues)
        }),
    );
    put("issue", Some(caught_issue(get("issue"))));
    for key in ["createdAt", "updatedAt"] {
        put(
            key,
            string(get(key), &at(path, key), &[Rule::Datetime], issues).map(Value::from),
        );
    }
}

/// `.default(value)`: undefined takes the default; anything else is parsed.
pub fn defaulted(
    value: Option<&Value>,
    default: Value,
    parse: impl FnOnce(&Value) -> Option<Value>,
) -> Option<Value> {
    match value {
        None => Some(default),
        Some(value) => parse(value),
    }
}

/// `.nullable().default(null)`.
pub fn nullable(
    value: Option<&Value>,
    parse: impl FnOnce(&Value) -> Option<Value>,
) -> Option<Value> {
    match value {
        None | Some(Value::Null) => Some(Value::Null),
        Some(value) => parse(value),
    }
}

/// `z.string().trim().min(1).max(400)`.
pub fn title(value: Option<&Value>, path: &[Value], issues: &mut Issues) -> Option<Value> {
    string(
        value,
        path,
        &[Rule::Trim, Rule::Min(1), Rule::Max(400)],
        issues,
    )
    .map(Value::from)
}

/// `z.array(z.string()...)` of labels with the given checks.
pub fn labels(value: &Value, path: &[Value], rules: &[Rule], issues: &mut Issues) -> Option<Value> {
    array(
        Some(value),
        path,
        Length::default(),
        issues,
        |each, at_path, issues| string(Some(each), at_path, rules, issues).map(Value::from),
    )
    .map(Value::Array)
}

pub fn task_ids(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    array(
        Some(value),
        path,
        Length::default(),
        issues,
        |each, at_path, issues| string(Some(each), at_path, &TASK_ID, issues).map(Value::from),
    )
    .map(Value::Array)
}

fn pull_request_status(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    let map = object(Some(value), path, issues)?;
    let state = one_of(
        map.get("state"),
        &at(path, "state"),
        &PULL_REQUEST_STATES,
        issues,
    );
    let checks = one_of(
        map.get("checks"),
        &at(path, "checks"),
        &CHECK_STATES,
        issues,
    );
    Some(json!({ "state": state?, "checks": checks? }))
}

/// `pr`: a bad link reads as none (`.catch(null)`).
fn caught_url(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(_)) => quietly(|quiet| string(value, &[], &[Rule::HttpUrl], quiet))
            .map_or(Value::Null, Value::from),
        _ => Value::Null,
    }
}

/// `issue`: a bad link reads as none.
fn caught_issue(value: Option<&Value>) -> Value {
    match value {
        None | Some(Value::Null) => Value::Null,
        Some(value) => quietly(|quiet| issue_link(value, &[], quiet)).unwrap_or(Value::Null),
    }
}

/// `issueLinkSchema`.
pub fn issue_link(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    let map = object(Some(value), path, issues)?;
    let provider = string(
        map.get("provider"),
        &at(path, "provider"),
        &[Rule::Trim, Rule::Min(1), Rule::Max(40)],
        issues,
    );
    let key = string(
        map.get("key"),
        &at(path, "key"),
        &[Rule::Trim, Rule::Min(1), Rule::Max(100)],
        issues,
    );
    let url = string(map.get("url"), &at(path, "url"), &[Rule::HttpUrl], issues);
    Some(json!({ "provider": provider?, "key": key?, "url": url? }))
}

/// `taskContextSchema`.
pub fn task_context(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    let map = object(Some(value), path, issues)?;
    let before = issues.len();
    let source = one_of(
        map.get("source"),
        &at(path, "source"),
        &CONTEXT_SOURCE_TYPES,
        issues,
    );
    let mut out = Map::new();
    if let Some(source) = source {
        out.insert("source".into(), source.into());
    }
    for (key, max) in [("name", 60), ("skill", 200)] {
        if let Some(each) = map.get(key) {
            let rules = [Rule::Trim, Rule::Min(1), Rule::Max(max)];
            if let Some(text) = string(Some(each), &at(path, key), &rules, issues) {
                out.insert(key.into(), text.into());
            }
        }
    }
    let reference = match map.get("ref") {
        None => Some(String::new()),
        Some(each) => string(
            Some(each),
            &at(path, "ref"),
            &[Rule::Trim, Rule::Max(500)],
            issues,
        ),
    };
    if let Some(reference) = reference {
        out.insert("ref".into(), reference.into());
    }
    (issues.len() == before).then_some(Value::Object(out))
}

/// `upgradeTaskFields`: a task written before stages existed gets its stage from its state, and
/// an issue import line in its notes becomes its issue link.
pub fn upgrade_task_fields(mut raw: Map<String, Value>) -> Map<String, Value> {
    if raw.contains_key("stage") {
        return raw;
    }
    let stage = match raw.get("state").and_then(Value::as_str) {
        Some("review") => "review",
        Some("done") => "release",
        _ => "implement",
    };
    let imported = raw
        .get("description")
        .and_then(Value::as_str)
        .and_then(|text| imported_issue(crate::js::js_trim(text)));
    raw.insert("stage".into(), stage.into());
    raw.insert("stages".into(), legacy_step_refs());
    if let (false, Some((url, number))) = (raw.contains_key("issue"), imported) {
        raw.insert(
            "issue".into(),
            json!({ "provider": "github", "key": format!("#{number}"), "url": url }),
        );
    }
    raw
}

/// `/Imported from (https:\/\/github\.com\/[^\s]+\/issues\/(\d+))\s*$/` on trimmed notes.
fn imported_issue(text: &str) -> Option<(String, String)> {
    let start = text.rfind("Imported from https://github.com/")?;
    let url = &text[start + "Imported from ".len()..];
    if url.chars().any(crate::js::is_js_space) {
        return None;
    }
    let (_, number) = url.rsplit_once("/issues/")?;
    let middle = &url["https://github.com/".len()..url.len() - number.len() - "/issues/".len()];
    let ok = !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && !middle.is_empty();
    ok.then(|| (url.to_string(), number.to_string()))
}

/// `splitTaskId`: "ABC-12: Fix it" picks the id ABC-12, in capitals.
pub fn split_task_id(title: &str) -> (Option<String>, String) {
    let trimmed = crate::js::js_trim(title);
    let letters = trimmed.bytes().take_while(u8::is_ascii_alphabetic).count();
    let rest = &trimmed[letters..];
    let digits = rest.strip_prefix('-').map_or(0, |after| {
        after.bytes().take_while(u8::is_ascii_digit).count()
    });
    if letters == 0 || digits == 0 {
        return (None, trimmed.to_string());
    }
    let id_len = letters + 1 + digits;
    let after = &trimmed[id_len..];
    // `(?:\s*:|\s)\s*(.*)$`: a colon after spaces, or one space; `.` stops at a line break.
    let spaced = after.trim_start_matches(crate::js::is_js_space);
    let body = if let Some(colon) = spaced.strip_prefix(':') {
        colon.trim_start_matches(crate::js::is_js_space)
    } else if after.starts_with(crate::js::is_js_space) {
        spaced
    } else {
        return (None, trimmed.to_string());
    };
    if body.is_empty() || body.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return (None, trimmed.to_string());
    }
    (
        Some(trimmed[..id_len].to_ascii_uppercase()),
        body.to_string(),
    )
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod tests;
