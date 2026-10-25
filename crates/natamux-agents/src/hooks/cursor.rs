//! The adapter of the Cursor CLI (`cursor-hooks.ts`). The folder is in `workspace_roots`; only
//! tool events have `cwd`.

use std::path::Path;

use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::fields;
use super::{HookResult, NormalizedHook, Signal, tool_summary};

// The Cursor CLI has no event for a wait on the user's approval: `beforeShellExecution` comes
// before the Run or Reject card, but also before commands that run without one. The terminal bell
// covers the wait instead.
fn signal(event: &str, status: Option<&str>) -> Option<Signal> {
    match event {
        "beforeSubmitPrompt" => Some(Signal::Prompt),
        "preToolUse"
        | "postToolUse"
        | "postToolUseFailure"
        | "beforeShellExecution"
        | "afterShellExecution"
        | "afterFileEdit" => Some(Signal::Activity),
        "stop" if status == Some("error") => Some(Signal::Failed),
        "stop" => Some(Signal::TurnEnded),
        _ => None,
    }
}

pub fn normalize(raw: &Value, _home: &Path) -> HookResult {
    let mut issues = Issues::default();
    let Some(map) = zod::object(Some(raw), &[], &mut issues) else {
        return Err(ZodError(issues.0));
    };
    let event = fields::event_name(map, "hook_event_name", &mut issues);
    let cwd = fields::text(map, "cwd", &mut issues);
    let roots = fields::strings(map, "workspace_roots", &mut issues);
    let conversation_id = fields::text(map, "conversation_id", &mut issues);
    let status = fields::text(map, "status", &mut issues);
    let tool_name = fields::text(map, "tool_name", &mut issues);
    let command = fields::text(map, "command", &mut issues);
    // `file_path` is read by the schema, but nothing uses it.
    fields::text(map, "file_path", &mut issues);
    issues.into_result(Some(()))?;
    let Some(signal) = signal(&event, status.as_deref()) else {
        return Ok(None);
    };
    let cwd = cwd.or_else(|| roots.and_then(|roots| roots.into_iter().next()));
    let summary = match event.as_str() {
        "beforeShellExecution" => {
            let mut input = Map::new();
            if let Some(command) = &command {
                input.insert("command".into(), json!(command));
            }
            tool_summary(Some("Shell"), Some(&input), cwd.as_deref())
        }
        "preToolUse" => {
            let input = fields::object_or_empty(map.get("tool_input"));
            tool_summary(tool_name.as_deref(), Some(&input), cwd.as_deref())
        }
        "beforeSubmitPrompt" => Some("Reading the prompt".to_string()),
        _ => None,
    };
    let mut details = Map::new();
    details.insert("hook".into(), json!(event));
    if let Some(id) = conversation_id {
        details.insert("sessionId".into(), json!(id));
    }
    if let Some(status) = status {
        details.insert("status".into(), json!(status));
    }
    Ok(Some(NormalizedHook {
        signal,
        cwd,
        details,
        summary,
        transcript: None,
    }))
}
