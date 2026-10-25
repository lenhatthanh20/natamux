//! The adapter of Claude Code (`claude-code-hooks.ts`).

use std::path::Path;

use natamux_core::zod::{self, Issues, ZodError};
use serde_json::{Map, Value, json};

use super::fields;
use super::{HookResult, NormalizedHook, SUMMARY_LENGTH, Signal, shorten, slice_utf16};

const MAX_MESSAGE_LENGTH: usize = 500;
const MAX_WAITING_ENTRIES: usize = 5;
const MAX_WAITING_LENGTH: usize = 200;

// Notification types that mean Claude waits for the user.
const ASKING_NOTIFICATIONS: [&str; 3] = [
    "permission_prompt",
    "elicitation_dialog",
    "agent_needs_input",
];

// Not checked against a real session yet; any other status counts as still running.
const FINISHED_STATUSES: [&str; 4] = ["completed", "failed", "killed", "stopped"];

/// The part of the JSON that Claude Code sends to a hook which Natamux reads.
struct Input {
    event: String,
    cwd: Option<String>,
    session_id: Option<String>,
    transcript_path: Option<String>,
    notification_type: Option<String>,
    message: Option<String>,
    tool_name: Option<String>,
    tool_input: Option<Map<String, Value>>,
    // Read entry by entry in `background_work`, so a bad entry does not lose the hook.
    background_tasks: Option<Value>,
    session_crons: Option<Value>,
}

fn read(map: &Map<String, Value>, issues: &mut Issues) -> Input {
    Input {
        event: fields::event_name(map, "hook_event_name", issues),
        cwd: fields::text(map, "cwd", issues),
        session_id: fields::text(map, "session_id", issues),
        transcript_path: fields::text(map, "transcript_path", issues),
        notification_type: fields::text(map, "notification_type", issues),
        message: fields::text(map, "message", issues),
        tool_name: fields::text(map, "tool_name", issues),
        tool_input: fields::record(map, "tool_input", issues),
        background_tasks: map.get("background_tasks").cloned(),
        session_crons: map.get("session_crons").cloned(),
    }
}

/// An optional string field: Some(None) when absent, None when it has another type, so the whole
/// entry is skipped.
fn optional<'a>(map: &'a Map<String, Value>, key: &str) -> Option<Option<&'a str>> {
    match map.get(key) {
        None => Some(None),
        Some(Value::String(text)) => Some(Some(text)),
        Some(_) => None,
    }
}

fn task_text(entry: &Value) -> Option<String> {
    let map = entry.as_object()?;
    let kind = map.get("type")?.as_str().filter(|kind| !kind.is_empty())?;
    let status = optional(map, "status")?;
    let description = optional(map, "description")?;
    let command = optional(map, "command")?;
    if status.is_some_and(|status| FINISHED_STATUSES.contains(&status)) {
        return None;
    }
    // Claude Code runs a background shell with its Bash tool, so it reads like a Bash summary.
    let name = if kind == "shell" { "Bash" } else { kind };
    match command.or(description).filter(|what| !what.is_empty()) {
        Some(what) => Some(format!("{name}: {what}")),
        None => Some(name.to_string()),
    }
}

fn cron_text(entry: &Value) -> Option<String> {
    let map = entry.as_object()?;
    let prompt = optional(map, "prompt")?;
    let schedule = optional(map, "schedule")?;
    match prompt.or(schedule).filter(|what| !what.is_empty()) {
        Some(what) => Some(format!("scheduled prompt: {what}")),
        None => Some("scheduled prompt".to_string()),
    }
}

fn entries(list: Option<&Value>, read: fn(&Value) -> Option<String>) -> Vec<String> {
    match list {
        Some(Value::Array(items)) => items.iter().filter_map(read).collect(),
        _ => Vec::new(),
    }
}

/// What a stopping session still has in flight, such as "Bash: pnpm test". Each of these wakes
/// Claude Code again when it ends or fires, so the session is paused, not done.
fn background_work(input: &Input) -> Vec<String> {
    let mut work = entries(input.background_tasks.as_ref(), task_text);
    work.extend(entries(input.session_crons.as_ref(), cron_text));
    work
}

// "Waiting for Bash: pnpm test and 2 more", cut to the length of a summary.
fn waiting_summary(waiting_for: &[String]) -> String {
    let more = if waiting_for.len() > 1 {
        format!(" and {} more", waiting_for.len() - 1)
    } else {
        String::new()
    };
    let head = format!("Waiting for {}", waiting_for[0]);
    format!(
        "{}{more}",
        shorten(&head, SUMMARY_LENGTH.saturating_sub(more.len()))
    )
}

fn signal(input: &Input, waiting: bool) -> Option<Signal> {
    match input.event.as_str() {
        "UserPromptSubmit" => Some(Signal::Prompt),
        "PreToolUse" | "PostToolUse" => Some(Signal::Activity),
        "PermissionRequest" => Some(Signal::Asking),
        "Notification" => input
            .notification_type
            .as_deref()
            .is_some_and(|kind| ASKING_NOTIFICATIONS.contains(&kind))
            .then_some(Signal::Asking),
        "Stop" if waiting => Some(Signal::TurnPaused),
        "Stop" => Some(Signal::TurnEnded),
        "StopFailure" => Some(Signal::Failed),
        _ => None,
    }
}

fn details(input: &Input, waiting_for: &[String]) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("hook".into(), json!(input.event));
    if let Some(id) = &input.session_id {
        details.insert("sessionId".into(), json!(id));
    }
    if let Some(kind) = &input.notification_type {
        details.insert("notificationType".into(), json!(kind));
    }
    if let Some(message) = &input.message {
        details.insert(
            "message".into(),
            json!(slice_utf16(message, MAX_MESSAGE_LENGTH)),
        );
    }
    if !waiting_for.is_empty() {
        let shown: Vec<String> = waiting_for
            .iter()
            .take(MAX_WAITING_ENTRIES)
            .map(|entry| shorten(entry, MAX_WAITING_LENGTH))
            .collect();
        details.insert("waitingFor".into(), json!(shown));
    }
    details
}

pub fn normalize(raw: &Value, _home: &Path) -> HookResult {
    let mut issues = Issues::default();
    let Some(map) = zod::object(Some(raw), &[], &mut issues) else {
        return Err(ZodError(issues.0));
    };
    let input = read(map, &mut issues);
    let input = issues.into_result(Some(input))?;
    let waiting_for = if input.event == "Stop" {
        background_work(&input)
    } else {
        Vec::new()
    };
    let Some(signal) = signal(&input, !waiting_for.is_empty()) else {
        return Ok(None);
    };
    // The tool that is about to run is what the agent does now.
    let summary = match input.event.as_str() {
        "PreToolUse" => super::tool_summary(
            input.tool_name.as_deref(),
            input.tool_input.as_ref(),
            input.cwd.as_deref(),
        ),
        "UserPromptSubmit" => Some("Reading the prompt".to_string()),
        _ if !waiting_for.is_empty() => Some(waiting_summary(&waiting_for)),
        _ => None,
    };
    Ok(Some(NormalizedHook {
        signal,
        details: details(&input, &waiting_for),
        cwd: input.cwd,
        summary,
        transcript: input.transcript_path,
    }))
}

#[cfg(test)]
#[path = "claude_code_tests.rs"]
mod tests;
