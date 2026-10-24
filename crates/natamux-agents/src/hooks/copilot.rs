//! The adapter of GitHub Copilot CLI (`copilot-hooks.ts`), in its camelCase form.

use std::path::Path;

use natamux_core::json::parse_js;
use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::fields;
use super::{HookResult, NormalizedHook, Signal, tool_summary};

/// The event a hook of Copilot belongs to. Copilot does not name it in the JSON, so
/// `natamux hook copilot <event>` adds it.
pub const COPILOT_EVENT_FIELD: &str = "natamux_event";

// The notifications that mean a prompt is on the screen, with what the pane says meanwhile.
fn asking_text(kind: &str) -> Option<&'static str> {
    match kind {
        "permission_prompt" => Some("Waiting for permission"),
        "elicitation_dialog" => Some("Asking a question"),
        _ => None,
    }
}

// Copilot 1.0.98 sends snake_case and other payloads in camelCase; a value of another type gives
// no signal instead of failing the hook. The snake_case text wins.
fn asking_summary(map: &Map<String, Value>) -> Option<&'static str> {
    let kind = ["notification_type", "notificationType"]
        .iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str))?;
    asking_text(kind)
}

fn signal(event: &str, map: &Map<String, Value>, recoverable: Option<bool>) -> Option<Signal> {
    match event {
        "userPromptSubmitted" => Some(Signal::Prompt),
        "preToolUse" | "postToolUse" | "postToolUseFailure" => Some(Signal::Activity),
        // Not permissionRequest: it fires before the permission rules, so also for each tool call
        // that --allow-all-tools approves at once.
        "notification" => asking_summary(map).map(|_| Signal::Asking),
        "agentStop" => Some(Signal::TurnEnded),
        // An error Copilot gets over (a retried model call) does not end the work.
        "errorOccurred" => (recoverable == Some(false)).then_some(Signal::Failed),
        _ => None,
    }
}

// A JSON text or an object, depending on the tool.
fn tool_args(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::String(text)) => match parse_js(text) {
            Ok(Value::Object(map)) => map,
            _ => Map::new(),
        },
        other => fields::object_or_empty(other),
    }
}

// Copilot sends no transcript path; its session's events, with the usage, are kept here.
fn events_file(session_id: Option<&str>, home: &Path) -> Option<String> {
    let id = session_id.filter(|id| {
        !id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    })?;
    let path = home
        .join(".copilot")
        .join("session-state")
        .join(id)
        .join("events.jsonl");
    Some(path.to_string_lossy().into_owned())
}

pub fn normalize(raw: &Value, home: &Path) -> HookResult {
    let mut issues = Issues::default();
    let Some(map) = zod::object(Some(raw), &[], &mut issues) else {
        return Err(ZodError(issues.0));
    };
    let event = fields::event_name(map, COPILOT_EVENT_FIELD, &mut issues);
    let cwd = fields::text(map, "cwd", &mut issues);
    let session_id = fields::text(map, "sessionId", &mut issues);
    let tool_name = fields::text(map, "toolName", &mut issues);
    let recoverable = fields::flag(map, "recoverable", &mut issues);
    issues.into_result(Some(()))?;
    let Some(signal) = signal(&event, map, recoverable) else {
        return Ok(None);
    };
    let summary = match event.as_str() {
        "preToolUse" => {
            let input = tool_args(map.get("toolArgs"));
            tool_summary(tool_name.as_deref(), Some(&input), cwd.as_deref())
        }
        "userPromptSubmitted" => Some("Reading the prompt".to_string()),
        "notification" => asking_summary(map).map(str::to_string),
        _ => None,
    };
    let transcript = events_file(session_id.as_deref(), home);
    let mut details = Map::new();
    details.insert("hook".into(), json!(event));
    if let Some(id) = session_id {
        details.insert("sessionId".into(), json!(id));
    }
    Ok(Some(NormalizedHook {
        signal,
        cwd,
        details,
        summary,
        transcript,
    }))
}
