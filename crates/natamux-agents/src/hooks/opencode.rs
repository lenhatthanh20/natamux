//! The adapter of OpenCode. Its hooks are a JS plugin (`hook_files::opencode_plugin`) that runs
//! `natamux hook opencode <event>` with this JSON on stdin:
//! `{ "sessionID", "cwd", "tool", "args", "error" }`, all optional. The event is the argument, as
//! for Copilot, in a form the CLI accepts (no dots).

use std::path::Path;

use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::copilot::COPILOT_EVENT_FIELD;
use super::fields;
use super::{HookResult, NormalizedHook, Signal, input_runs_ask, slice_utf16, tool_summary};

fn signal(event: &str) -> Option<Signal> {
    match event {
        "chatMessage" => Some(Signal::Prompt),
        "toolBefore" | "toolAfter" => Some(Signal::Activity),
        "permissionAsked" | "permissionV2Asked" => Some(Signal::Asking),
        "sessionIdle" => Some(Signal::TurnEnded),
        "sessionError" => Some(Signal::Failed),
        _ => None,
    }
}

pub fn normalize(raw: &Value, _home: &Path) -> HookResult {
    let mut issues = Issues::default();
    let Some(map) = zod::object(Some(raw), &[], &mut issues) else {
        return Err(ZodError(issues.0));
    };
    let event = fields::event_name(map, COPILOT_EVENT_FIELD, &mut issues);
    let cwd = fields::text(map, "cwd", &mut issues);
    let session_id = fields::text(map, "sessionID", &mut issues);
    let tool = fields::text(map, "tool", &mut issues);
    let error = fields::text(map, "error", &mut issues);
    let args = fields::record(map, "args", &mut issues);
    issues.into_result(Some(()))?;
    if event == "toolAfter" && input_runs_ask(args.as_ref()) {
        return Ok(None);
    }
    let Some(signal) = signal(&event) else {
        return Ok(None);
    };
    let summary = match event.as_str() {
        "toolBefore" => tool_summary(tool.as_deref(), args.as_ref(), cwd.as_deref()),
        "chatMessage" => Some("Reading the prompt".to_string()),
        _ => None,
    };
    let mut details = Map::new();
    details.insert("hook".into(), json!(event));
    if let Some(id) = session_id {
        details.insert("sessionId".into(), json!(id));
    }
    if let Some(error) = error {
        details.insert("message".into(), json!(slice_utf16(&error, 500)));
    }
    Ok(Some(NormalizedHook {
        signal,
        cwd,
        details,
        summary,
        transcript: None,
    }))
}
