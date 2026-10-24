//! The adapter of Gemini CLI (`gemini-hooks.ts`).

use std::path::Path;

use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::fields;
use super::{HookResult, NormalizedHook, Signal, slice_utf16, tool_summary};

// Gemini CLI has no event for a failed turn; a turn that ends in an error leaves the task running
// until the quiet rule or the user notices.
fn signal(event: &str, notification_type: Option<&str>) -> Option<Signal> {
    match event {
        "BeforeAgent" => Some(Signal::Prompt),
        "BeforeTool" | "AfterTool" => Some(Signal::Activity),
        "Notification" => (notification_type == Some("ToolPermission")).then_some(Signal::Asking),
        "AfterAgent" => Some(Signal::TurnEnded),
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
    let tool_name = fields::text(map, "tool_name", &mut issues);
    let tool_input = fields::record(map, "tool_input", &mut issues);
    issues.into_result(Some(()))?;
    let Some(signal) = signal(&event, notification_type.as_deref()) else {
        return Ok(None);
    };
    let summary = match event.as_str() {
        "BeforeTool" => tool_summary(tool_name.as_deref(), tool_input.as_ref(), cwd.as_deref()),
        "BeforeAgent" => Some("Reading the prompt".to_string()),
        _ => None,
    };
    let mut details = Map::new();
    details.insert("hook".into(), json!(event));
    if let Some(id) = session_id {
        details.insert("sessionId".into(), json!(id));
    }
    if let Some(message) = message {
        details.insert("message".into(), json!(slice_utf16(&message, 500)));
    }
    Ok(Some(NormalizedHook {
        signal,
        cwd,
        details,
        summary,
        transcript,
    }))
}
