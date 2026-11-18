//! The adapter of Qwen Code: a Gemini CLI fork whose hooks have Claude Code's event names.

use std::path::Path;

use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::fields;
use super::{HookResult, NormalizedHook, Signal, input_runs_ask, slice_utf16, tool_summary};

fn signal(event: &str, notification_type: Option<&str>) -> Option<Signal> {
    match event {
        "UserPromptSubmit" => Some(Signal::Prompt),
        "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => Some(Signal::Activity),
        "PermissionRequest" => Some(Signal::Asking),
        "Notification" => {
            (notification_type == Some("permission_prompt")).then_some(Signal::Asking)
        }
        "Stop" => Some(Signal::TurnEnded),
        "StopFailure" => Some(Signal::Failed),
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
    let session_id = fields::text(map, "session_id", &mut issues);
    let transcript = fields::text(map, "transcript_path", &mut issues);
    let notification_type = fields::text(map, "notification_type", &mut issues);
    let message = fields::text(map, "message", &mut issues);
    let error = fields::text(map, "error", &mut issues);
    let error_details = fields::text(map, "error_details", &mut issues);
    let tool_name = fields::text(map, "tool_name", &mut issues);
    let tool_input = fields::record(map, "tool_input", &mut issues);
    issues.into_result(Some(()))?;
    if event == "PostToolUse" && input_runs_ask(tool_input.as_ref()) {
        return Ok(None);
    }
    let Some(signal) = signal(&event, notification_type.as_deref()) else {
        return Ok(None);
    };
    let summary = match event.as_str() {
        "PreToolUse" => tool_summary(tool_name.as_deref(), tool_input.as_ref(), cwd.as_deref()),
        "UserPromptSubmit" => Some("Reading the prompt".to_string()),
        _ => None,
    };
    let mut details = Map::new();
    details.insert("hook".into(), json!(event));
    if let Some(id) = session_id {
        details.insert("sessionId".into(), json!(id));
    }
    let text = if signal == Signal::Failed {
        error_details.or(error).or(message)
    } else {
        message
    };
    if let Some(text) = text {
        details.insert("message".into(), json!(slice_utf16(&text, 500)));
    }
    Ok(Some(NormalizedHook {
        signal,
        cwd,
        details,
        summary,
        transcript,
    }))
}
