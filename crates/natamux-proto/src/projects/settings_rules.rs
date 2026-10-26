//! The rules of `projectSettingsSchema` outside `stages` (`packages/core/src/projects/
//! project-settings.ts`), as the daemon words them: `<path>: <message>`, joined by "; ". The
//! rules of `stages` are in `stages/pipeline_rules.rs`.

use serde_json::{Map, Value};

/// The first problem line of the settings in `raw` that is not about `stages`, or None.
pub fn other_settings_problem(raw: &Map<String, Value>) -> Option<String> {
    let mut found = Vec::new();
    if let Some(context) = object(raw.get("context"), "context", &mut found) {
        sources(context.get("sources"), &mut found);
    }
    if let Some(tracker) = object(raw.get("tracker"), "tracker", &mut found) {
        one_of(
            tracker.get("type"),
            "tracker.type",
            &["github", "none"],
            &mut found,
        );
        let when = ["never", "manual", "on-spec-approved", "on-pr"];
        one_of(
            tracker.get("createIssue"),
            "tracker.createIssue",
            &when,
            &mut found,
        );
        match tracker.get("linkInPr") {
            None | Some(Value::Bool(_)) => {}
            Some(other) => found.push(wrong_type("tracker.linkInPr", "boolean", other)),
        }
    }
    if let Some(agents) = object(raw.get("agents"), "agents", &mut found) {
        let path = "agents.permissionMode";
        one_of(
            agents.get("permissionMode"),
            path,
            &["auto", "ask"],
            &mut found,
        );
        count(
            agents.get("maxSendBacks"),
            "agents.maxSendBacks",
            &mut found,
        );
    }
    (!found.is_empty()).then(|| found.join("; "))
}

/// `z.number().int().min(0).optional()`. shortcut: zod takes `2.0` as an int; this refuses it,
/// as serde cannot read it into a count.
fn count(value: Option<&Value>, path: &str, found: &mut Vec<String>) {
    match value {
        None => {}
        Some(Value::Number(number)) if number.is_u64() => {}
        Some(Value::Number(number)) if number.as_f64().is_some_and(|n| n < 0.0) => {
            found.push(format!("{path}: Too small: expected number to be >=0"));
        }
        Some(Value::Number(_)) => {
            found.push(format!(
                "{path}: Invalid input: expected int, received number"
            ));
        }
        Some(other) => found.push(wrong_type(path, "number", other)),
    }
}

fn kind_of(value: Option<&Value>) -> &'static str {
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

fn wrong_type(path: &str, expected: &str, value: &Value) -> String {
    let received = kind_of(Some(value));
    format!("{path}: Invalid input: expected {expected}, received {received}")
}

/// A field with a default: left out is fine, anything but an object is not.
fn object<'a>(
    value: Option<&'a Value>,
    path: &str,
    found: &mut Vec<String>,
) -> Option<&'a Map<String, Value>> {
    match value {
        None => None,
        Some(Value::Object(map)) => Some(map),
        Some(other) => {
            found.push(wrong_type(path, "object", other));
            None
        }
    }
}

fn one_of(value: Option<&Value>, path: &str, options: &[&str], found: &mut Vec<String>) {
    let Some(value) = value else {
        return;
    };
    if !value.as_str().is_some_and(|text| options.contains(&text)) {
        let list = options
            .iter()
            .map(|option| format!("\"{option}\""))
            .collect::<Vec<_>>()
            .join("|");
        found.push(format!("{path}: Invalid option: expected one of {list}"));
    }
}

fn sources(value: Option<&Value>, found: &mut Vec<String>) {
    let list = match value {
        None => return,
        Some(Value::Array(list)) => list,
        Some(other) => {
            found.push(wrong_type("context.sources", "array", other));
            return;
        }
    };
    for (index, source) in list.iter().enumerate() {
        let path = format!("context.sources.{index}");
        let Value::Object(map) = source else {
            found.push(wrong_type(&path, "object", source));
            continue;
        };
        match map.get("type").and_then(Value::as_str) {
            Some("manual" | "github-issue") => {}
            Some("skill") => {
                text(map.get("name"), &format!("{path}.name"), Some(60), found);
                text(map.get("skill"), &format!("{path}.skill"), None, found);
                agent(map.get("agent"), &path, found);
            }
            Some("mcp") => {
                text(map.get("name"), &format!("{path}.name"), Some(60), found);
                text(map.get("prompt"), &format!("{path}.prompt"), None, found);
                agent(map.get("agent"), &path, found);
            }
            _ => found.push(format!("{path}.type: Invalid input")),
        }
    }
    if list.is_empty() {
        found.push("context.sources: Give at least one context source".into());
    }
}

/// `z.string().trim().min(1)`, with `.max(limit)` when there is one: required.
fn text(value: Option<&Value>, path: &str, limit: Option<usize>, found: &mut Vec<String>) {
    let Some(Value::String(text)) = value else {
        found.push(wrong_type_of(path, "string", value));
        return;
    };
    let length = text.trim().encode_utf16().count();
    if length == 0 {
        found.push(format!(
            "{path}: Too small: expected string to have >=1 characters"
        ));
    } else if limit.is_some_and(|limit| length > limit) {
        let limit = limit.unwrap_or_default();
        found.push(format!(
            "{path}: Too big: expected string to have <={limit} characters"
        ));
    }
}

fn wrong_type_of(path: &str, expected: &str, value: Option<&Value>) -> String {
    let received = kind_of(value);
    format!("{path}: Invalid input: expected {expected}, received {received}")
}

/// `z.string().min(1).optional()`: the agent of a source.
fn agent(value: Option<&Value>, path: &str, found: &mut Vec<String>) {
    let path = format!("{path}.agent");
    match value {
        None => {}
        Some(Value::String(text)) if !text.is_empty() => {}
        Some(Value::String(_)) => {
            found.push(format!(
                "{path}: Too small: expected string to have >=1 characters"
            ));
        }
        other => found.push(wrong_type_of(&path, "string", other)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::projects::repo::{ContextSettings, TrackerSettings};

    fn problem(value: Value) -> Option<String> {
        other_settings_problem(value.as_object().unwrap())
    }

    #[test]
    fn needs_at_least_one_context_source() {
        assert_eq!(
            problem(json!({"context": {"sources": []}})).as_deref(),
            Some("context.sources: Give at least one context source")
        );
    }

    #[test]
    fn fills_every_field_of_an_empty_file() {
        assert_eq!(problem(json!({})), None);
        // The pipeline default (DEFAULT_PIPELINE) is filled by the daemon: `stages` stays empty.
        let sources = ContextSettings::default().sources;
        let kinds: Vec<&str> = sources.iter().map(|source| source.kind.as_str()).collect();
        assert_eq!(kinds, ["manual", "github-issue"]);
        let tracker = TrackerSettings::default();
        assert_eq!(
            (
                tracker.kind.as_str(),
                tracker.create_issue.as_str(),
                tracker.link_in_pr
            ),
            ("github", "manual", true)
        );
    }

    #[test]
    fn opens_issues_only_when_asked_unless_the_file_says_otherwise() {
        for tracker in [json!({}), json!({"type": "github"})] {
            assert_eq!(problem(json!({"tracker": tracker})), None);
        }
        assert_eq!(TrackerSettings::default().create_issue, "manual");
        for value in ["never", "manual", "on-spec-approved", "on-pr"] {
            assert_eq!(problem(json!({"tracker": {"createIssue": value}})), None);
        }
        assert_eq!(
            problem(json!({"tracker": {"createIssue": "always"}})).as_deref(),
            Some(
                "tracker.createIssue: Invalid option: expected one of \"never\"|\"manual\"|\"on-spec-approved\"|\"on-pr\""
            )
        );
    }

    #[test]
    fn takes_auto_or_ask_as_the_permission_mode_of_the_agents() {
        assert_eq!(problem(json!({"agents": {}})), None);
        assert_eq!(problem(json!({"agents": {"permissionMode": "ask"}})), None);
        assert_eq!(
            problem(json!({"agents": {"permissionMode": "yolo"}})).as_deref(),
            Some("agents.permissionMode: Invalid option: expected one of \"auto\"|\"ask\"")
        );
    }

    #[test]
    fn takes_a_whole_number_of_zero_or_more_as_the_most_send_backs() {
        for good in [json!(0), json!(3)] {
            assert_eq!(problem(json!({"agents": {"maxSendBacks": good}})), None);
        }
        let wrong = |value: Value| problem(json!({"agents": {"maxSendBacks": value}}));
        assert_eq!(
            wrong(json!(-1)).as_deref(),
            Some("agents.maxSendBacks: Too small: expected number to be >=0")
        );
        assert_eq!(
            wrong(json!(1.5)).as_deref(),
            Some("agents.maxSendBacks: Invalid input: expected int, received number")
        );
        assert_eq!(
            wrong(json!("2")).as_deref(),
            Some("agents.maxSendBacks: Invalid input: expected number, received string")
        );
        assert_eq!(
            wrong(Value::Null).as_deref(),
            Some("agents.maxSendBacks: Invalid input: expected number, received null")
        );
    }

    #[test]
    fn reads_context_sources_by_type() {
        let sources = json!([
            {"type": "mcp", "name": "Jira", "prompt": "Read {ref} with the jira MCP server"},
            {"type": "skill", "name": "Ticket", "skill": "fetch-ticket"}
        ]);
        assert_eq!(problem(json!({"context": {"sources": sources}})), None);
        let missing = problem(json!({"context": {"sources": [{"type": "mcp", "name": "Jira"}]}}));
        assert_eq!(
            missing.as_deref(),
            Some("context.sources.0.prompt: Invalid input: expected string, received undefined")
        );
    }

    #[test]
    fn joins_the_problems_and_checks_the_type_of_a_source_and_a_flag() {
        let found = problem(json!({
            "context": {"sources": [{"type": "jira"}]},
            "tracker": {"type": "gitlab", "linkInPr": "yes"}
        }));
        assert_eq!(
            found.as_deref(),
            Some(
                "context.sources.0.type: Invalid input; tracker.type: Invalid option: expected one of \"github\"|\"none\"; tracker.linkInPr: Invalid input: expected boolean, received string"
            )
        );
    }
}
