//! The adapter of Codex (`codex-hooks.ts`). Its events and fields are named as Claude Code's.

use std::path::Path;

use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::fields;
use super::{HookResult, NormalizedHook, Signal, tool_summary};

fn signal(event: &str) -> Option<Signal> {
    match event {
        "UserPromptSubmit" => Some(Signal::Prompt),
        "PreToolUse" | "PostToolUse" => Some(Signal::Activity),
        "PermissionRequest" => Some(Signal::Asking),
        // Esc ends the turn too: Codex then waits for the next prompt.
        "Stop" | "Interrupt" => Some(Signal::TurnEnded),
        _ => None,
    }
}

// Codex runs a shell command as a list of words, often ["bash", "-lc", "<script>"].
fn command_text(command: Option<&Value>) -> Option<String> {
    match command? {
        Value::String(text) => Some(text.clone()),
        Value::Array(words) => {
            let words: Option<Vec<&str>> = words.iter().map(Value::as_str).collect();
            let words = words?;
            let flag = words
                .iter()
                .position(|word| *word == "-lc" || *word == "-c");
            match flag.and_then(|index| words.get(index + 1)) {
                Some(script) => Some((*script).to_string()),
                None => Some(words.join(" ")),
            }
        }
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
    // `turn_id` is read by the schema, but nothing uses it.
    fields::text(map, "turn_id", &mut issues);
    let tool_name = fields::text(map, "tool_name", &mut issues);
    issues.into_result(Some(()))?;
    let Some(signal) = signal(&event) else {
        return Ok(None);
    };
    let summary = match event.as_str() {
        "PreToolUse" => {
            let mut tool_input = fields::object_or_empty(map.get("tool_input"));
            if let Some(command) = command_text(tool_input.get("command")) {
                tool_input.insert("command".into(), json!(command));
            }
            tool_summary(tool_name.as_deref(), Some(&tool_input), cwd.as_deref())
        }
        "UserPromptSubmit" => Some("Reading the prompt".to_string()),
        _ => None,
    };
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
