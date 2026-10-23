//! The request bodies of the task routes (`task-inputs.ts`, `template-service.ts`, the zod
//! schemas of `routes/tasks.ts`), parsed as zod parses them.

use natamux_proto::events::task_event::{EVENT_SOURCES, EVENT_TYPES, RECORD_ONLY_EVENT_TYPES};
use natamux_proto::tasks::task::{TASK_PRIORITIES, TASK_STATES};
use serde_json::{Map, Value, json};

use crate::formats::{REPO_ID, TASK_ID, TEMPLATE_ID};
use crate::tasks::task::{defaulted, labels, nullable, task_context, task_ids, title};
use crate::zod::{
    Issue, Issues, Length, Rule, ZodError, array, at, object, one_of, strict, string,
};

fn parse_object(
    value: Option<&Value>,
    fields: impl FnOnce(&Map<String, Value>, &mut Map<String, Value>, &mut Issues),
) -> Result<Map<String, Value>, ZodError> {
    let mut issues = Issues::default();
    let mut out = Map::new();
    if let Some(map) = object(value, &[], &mut issues) {
        fields(map, &mut out, &mut issues);
    }
    issues.into_result(Some(out))
}

fn put(out: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        out.insert(key.into(), value);
    }
}

/// `createTaskInputSchema`: the fields a new task may bring, with their defaults.
pub fn create_task_input(value: Option<&Value>) -> Result<Map<String, Value>, ZodError> {
    parse_object(value, |map, out, issues| {
        let path = |key: &str| vec![json!(key)];
        put(
            out,
            "title",
            title(map.get("title"), &path("title"), issues),
        );
        let description = defaulted(map.get("description"), json!(""), |v| {
            string(Some(v), &path("description"), &[], issues).map(Value::from)
        });
        put(out, "description", description);
        let priority = defaulted(map.get("priority"), json!("normal"), |v| {
            one_of(Some(v), &path("priority"), &TASK_PRIORITIES, issues).map(Value::from)
        });
        put(out, "priority", priority);
        let labels_value = defaulted(map.get("labels"), json!([]), |v| {
            labels(v, &path("labels"), &[Rule::Min(1)], issues)
        });
        put(out, "labels", labels_value);
        let agent = nullable(map.get("agent"), |v| {
            string(Some(v), &path("agent"), &[Rule::Min(1)], issues).map(Value::from)
        });
        put(out, "agent", agent);
        let depends_on = defaulted(map.get("dependsOn"), json!([]), |v| {
            task_ids(v, &path("dependsOn"), issues)
        });
        put(out, "dependsOn", depends_on);
        let parent = nullable(map.get("parentId"), |v| {
            string(Some(v), &path("parentId"), &TASK_ID, issues).map(Value::from)
        });
        put(out, "parentId", parent);
        let repo = nullable(map.get("repo"), |v| {
            string(Some(v), &path("repo"), &REPO_ID, issues).map(Value::from)
        });
        put(out, "repo", repo);
        // `.nullable().optional()`: left out stays out.
        match map.get("context") {
            None => {}
            Some(Value::Null) => put(out, "context", Some(Value::Null)),
            Some(each) => put(out, "context", task_context(each, &path("context"), issues)),
        }
    })
}

/// `updateTaskInputSchema`: partial and strict; only the keys sent.
pub fn update_task_input(value: Option<&Value>) -> Result<Map<String, Value>, ZodError> {
    const SHAPE: [&str; 5] = ["title", "description", "priority", "labels", "dependsOn"];
    parse_object(value, |map, out, issues| {
        let path = |key: &str| vec![json!(key)];
        if let Some(each) = map.get("title") {
            put(out, "title", title(Some(each), &path("title"), issues));
        }
        if let Some(each) = map.get("description") {
            put(
                out,
                "description",
                string(Some(each), &path("description"), &[], issues).map(Value::from),
            );
        }
        if let Some(each) = map.get("priority") {
            put(
                out,
                "priority",
                one_of(Some(each), &path("priority"), &TASK_PRIORITIES, issues).map(Value::from),
            );
        }
        if let Some(each) = map.get("labels") {
            put(
                out,
                "labels",
                labels(each, &path("labels"), &[Rule::Trim, Rule::Min(1)], issues),
            );
        }
        if let Some(each) = map.get("dependsOn") {
            put(out, "dependsOn", task_ids(each, &path("dependsOn"), issues));
        }
        strict(map, &SHAPE, &[], issues);
    })
}

const REFINE_TEXT: &str = "This event is recorded by the daemon, not sent by clients";

/// `applyEventInputSchema`: source, type and payload, and no type that only the daemon records.
pub fn apply_event_input(value: Option<&Value>) -> Result<Map<String, Value>, ZodError> {
    let mut issues = Issues::default();
    let mut out = Map::new();
    if let Some(map) = object(value, &[], &mut issues) {
        let source = one_of(
            map.get("source"),
            &[json!("source")],
            &EVENT_SOURCES,
            &mut issues,
        );
        let kind = one_of(map.get("type"), &[json!("type")], &EVENT_TYPES, &mut issues);
        let payload = payload(map.get("payload"), &[json!("payload")], &mut issues);
        put(&mut out, "source", source.map(Value::from));
        put(&mut out, "type", kind.clone().map(Value::from));
        put(&mut out, "payload", payload);
        // A refine runs only when nothing before it stopped the parse.
        let refused = kind.is_some_and(|kind| RECORD_ONLY_EVENT_TYPES.contains(&kind.as_str()));
        if !issues.aborted_since(0) && refused {
            issues.push(Issue::custom(&[json!("type")], REFINE_TEXT));
        }
    }
    issues.into_result(Some(out))
}

/// `z.record(z.string(), z.unknown()).default({})`.
pub fn payload(value: Option<&Value>, path: &[Value], issues: &mut Issues) -> Option<Value> {
    match value {
        None => Some(json!({})),
        Some(Value::Object(map)) => Some(Value::Object(map.clone())),
        Some(other) => {
            issues.push(crate::zod::invalid_type("record", Some(other), path));
            None
        }
    }
}

/// `{ state: taskStateSchema.optional() }` of `GET /tasks`.
pub fn list_query(state: Option<&Value>) -> Result<Option<String>, ZodError> {
    let mut issues = Issues::default();
    let state = match state {
        None => None,
        Some(each) => one_of(Some(each), &[json!("state")], &TASK_STATES, &mut issues),
    };
    issues.into_result(Some(state))
}

/// `taskParamsSchema` and friends: one path parameter with its rules.
pub fn param(name: &str, value: &str, rules: &[Rule]) -> Result<String, ZodError> {
    let mut issues = Issues::default();
    let parsed = string(Some(&json!(value)), &[json!(name)], rules, &mut issues);
    issues.into_result(parsed)
}

pub fn task_id_param(value: &str) -> Result<String, ZodError> {
    param("id", value, &TASK_ID)
}

pub fn template_id_param(value: &str) -> Result<String, ZodError> {
    param("id", value, &TEMPLATE_ID)
}

/// `attachmentParamsSchema`: the id and the file name (1 to 255 characters).
pub fn attachment_params(id: &str, name: &str) -> Result<(String, String), ZodError> {
    let mut issues = Issues::default();
    let id = string(Some(&json!(id)), &[json!("id")], &TASK_ID, &mut issues);
    let name = string(
        Some(&json!(name)),
        &[json!("name")],
        &[Rule::Min(1), Rule::Max(255)],
        &mut issues,
    );
    issues.into_result(id.zip(name))
}

/// `{ repo: repoIdSchema.nullish() }`: the repository a new task names, if any.
pub fn task_repo_body(value: &Value) -> Result<Option<String>, ZodError> {
    let mut issues = Issues::default();
    let repo = object(Some(value), &[], &mut issues).and_then(|map| match map.get("repo") {
        None | Some(Value::Null) => Some(None),
        Some(each) => string(Some(each), &[json!("repo")], &REPO_ID, &mut issues).map(Some),
    });
    issues.into_result(repo)
}

/// A file sent with a new task: its name and base64 data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upload {
    pub name: String,
    pub data: String,
}

/// `{ attachments: attachmentUploadsSchema.optional() }`.
pub fn attachments_body(value: &Value) -> Result<Vec<Upload>, ZodError> {
    let max_data = crate::tasks::attachments::base64_length(
        natamux_proto::tasks::attachments::MAX_ATTACHMENT_SIZE,
    );
    let mut issues = Issues::default();
    let uploads = object(Some(value), &[], &mut issues).and_then(|map| {
        let Some(list) = map.get("attachments") else {
            return Some(Vec::new());
        };
        let length = Length {
            min: None,
            max: Some(natamux_proto::tasks::attachments::MAX_ATTACHMENT_COUNT),
        };
        array(
            Some(list),
            &[json!("attachments")],
            length,
            &mut issues,
            |each, path, issues| {
                let file = object(Some(each), path, issues)?;
                let name = string(
                    file.get("name"),
                    &at(path, "name"),
                    &[Rule::Min(1), Rule::Max(255)],
                    issues,
                );
                let data = string(
                    file.get("data"),
                    &at(path, "data"),
                    &[Rule::Max(max_data as usize)],
                    issues,
                );
                Some(Upload {
                    name: name?,
                    data: data?,
                })
            },
        )
    });
    issues.into_result(uploads)
}

/// `issueLinkSchema` as a body.
pub fn issue_link_body(value: Option<&Value>) -> Result<Value, ZodError> {
    let mut issues = Issues::default();
    let link = match value {
        Some(each) => crate::tasks::task::issue_link(each, &[], &mut issues),
        None => {
            issues.push(crate::zod::invalid_type("object", None, &[]));
            None
        }
    };
    issues.into_result(link)
}

/// `taskTemplateSchema` fields after the id: name, title, notes, priority, labels.
pub fn template_fields(
    map: &Map<String, Value>,
    out: &mut Map<String, Value>,
    issues: &mut Issues,
) {
    let path = |key: &str| vec![json!(key)];
    let name = string(
        map.get("name"),
        &path("name"),
        &[Rule::Trim, Rule::Min(1), Rule::Max(100)],
        issues,
    );
    put(out, "name", name.map(Value::from));
    let title = string(
        map.get("title"),
        &path("title"),
        &[Rule::Trim, Rule::Min(1), Rule::Max(200)],
        issues,
    );
    put(out, "title", title.map(Value::from));
    let description = defaulted(map.get("description"), json!(""), |v| {
        string(Some(v), &path("description"), &[], issues).map(Value::from)
    });
    put(out, "description", description);
    let priority = defaulted(map.get("priority"), json!("normal"), |v| {
        one_of(Some(v), &path("priority"), &TASK_PRIORITIES, issues).map(Value::from)
    });
    put(out, "priority", priority);
    let labels_value = defaulted(map.get("labels"), json!([]), |v| {
        labels(v, &path("labels"), &[Rule::Trim, Rule::Min(1)], issues)
    });
    put(out, "labels", labels_value);
}

/// `newTemplateInputSchema`: a template without its id.
pub fn template_input(value: Option<&Value>) -> Result<Map<String, Value>, ZodError> {
    parse_object(value, template_fields)
}

/// `taskTemplateSchema`: a template file (`id` first).
pub fn template(value: &Value) -> Result<Map<String, Value>, ZodError> {
    parse_object(Some(value), |map, out, issues| {
        let id = string(map.get("id"), &[json!("id")], &TEMPLATE_ID, issues);
        put(out, "id", id.map(Value::from));
        template_fields(map, out, issues);
    })
}

/// `fromTemplateInputSchema`: the title of the new task, and maybe its repository.
pub fn from_template_input(value: Option<&Value>) -> Result<(String, Option<String>), ZodError> {
    let mut issues = Issues::default();
    let parsed = object(value, &[], &mut issues).and_then(|map| {
        let title = string(
            map.get("title"),
            &[json!("title")],
            &[Rule::Trim, Rule::Min(1), Rule::Max(200)],
            &mut issues,
        );
        let repo = match map.get("repo") {
            None | Some(Value::Null) => Some(None),
            Some(each) => string(Some(each), &[json!("repo")], &REPO_ID, &mut issues).map(Some),
        };
        Some((title?, repo?))
    });
    issues.into_result(parsed)
}

#[cfg(test)]
#[path = "inputs_tests.rs"]
mod tests;
