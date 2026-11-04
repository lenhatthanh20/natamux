//! The step refs, lanes and paths of `taskSchema`.

use natamux_proto::tasks::task::TASK_STATES;
use serde_json::{Map, Value, json};

use crate::formats::STAGE_ID;
use crate::zod::{Issue, Issues, Length, array, at, one_of, quietly, string};

/// `z.array(stepRefSchema).min(1)`.
pub(crate) fn step_refs(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    let length = Length {
        min: Some(1),
        max: None,
    };
    array(
        Some(value),
        path,
        length,
        issues,
        |each, at_path, issues| {
            let parsed = step_ref(each);
            if parsed.is_none() {
                issues.push(Issue::new(
                    vec![("code", json!("invalid_union")), ("errors", json!([]))],
                    at_path,
                    "Invalid input".into(),
                ));
            }
            parsed
        },
    )
    .map(Value::Array)
}

fn stage_id(value: Option<&Value>, quiet: &mut Issues) -> Option<Value> {
    string(value, &[], &STAGE_ID, quiet).map(Value::from)
}

fn ids(value: Option<&Value>, length: Length, quiet: &mut Issues) -> Option<Vec<Value>> {
    array(value, &[], length, quiet, |each, _, quiet| {
        stage_id(Some(each), quiet)
    })
}

/// A stage id, or a group of 2 to 4 stage ids: what a path and the pipeline both hold.
fn stage_or_group(value: &Value) -> Option<Value> {
    if let Some(id) = quietly(|quiet| stage_id(Some(value), quiet)) {
        return Some(id);
    }
    let map = value.as_object()?;
    quietly(|quiet| {
        let id = stage_id(map.get("id"), quiet);
        let length = Length {
            min: Some(2),
            max: Some(4),
        };
        let parallel = ids(map.get("parallel"), length, quiet);
        Some(json!({ "id": id?, "parallel": parallel? }))
    })
}

/// The first option of `stepRefSchema` that takes the value, stripped to its keys.
fn step_ref(value: &Value) -> Option<Value> {
    if let Some(step) = stage_or_group(value) {
        return Some(step);
    }
    let map = value.as_object()?;
    quietly(|quiet| {
        let id = stage_id(map.get("id"), quiet);
        let length = Length {
            min: Some(2),
            max: Some(4),
        };
        let paths = array(map.get("paths"), &[], length, quiet, |each, _, quiet| {
            let path = each.as_object()?;
            let id = stage_id(path.get("id"), quiet);
            let one = Length {
                min: Some(1),
                max: None,
            };
            // A path holds stage ids and groups (a group runs side by side inside the path).
            let stages = array(
                path.get("stages"),
                &[],
                one,
                quiet,
                |item, at_path, quiet| {
                    let parsed = stage_or_group(item);
                    if parsed.is_none() {
                        quiet.push(Issue::new(Vec::new(), at_path, "Invalid input".into()));
                    }
                    parsed
                },
            );
            Some(json!({ "id": id?, "stages": stages? }))
        });
        Some(json!({ "id": id?, "paths": paths? }))
    })
}

/// `lanesSchema`: stage id to task state.
pub(crate) fn lanes(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    record(value, path, issues, |key, each, at_path, issues| {
        if !crate::formats::is_slug(key) || key.len() > 40 {
            issues.push(Issue::new(
                vec![("code", json!("invalid_key")), ("origin", json!("record"))],
                at_path,
                "Invalid key in record".into(),
            ));
            return None;
        }
        one_of(Some(each), at_path, &TASK_STATES, issues).map(Value::from)
    })
}

pub(crate) fn string_record(value: &Value, path: &[Value], issues: &mut Issues) -> Option<Value> {
    record(value, path, issues, |_, each, at_path, issues| {
        string(Some(each), at_path, &[], issues).map(Value::from)
    })
}

fn record(
    value: &Value,
    path: &[Value],
    issues: &mut Issues,
    mut entry: impl FnMut(&str, &Value, &[Value], &mut Issues) -> Option<Value>,
) -> Option<Value> {
    let Value::Object(map) = value else {
        issues.push(crate::zod::invalid_type("record", Some(value), path));
        return None;
    };
    let before = issues.len();
    let mut out = Map::new();
    for key in crate::json::js_keys(map) {
        if let Some(parsed) = entry(key, &map[key.as_str()], &at(path, key.as_str()), issues) {
            out.insert(key.clone(), parsed);
        }
    }
    (issues.len() == before).then_some(Value::Object(out))
}
