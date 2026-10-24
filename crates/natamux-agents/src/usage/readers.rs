//! Readers of the session files (`usage-readers.ts`). Only numbers and model names leave them; a
//! bad line is skipped. A file that cannot be read gives no usage.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::Path;

use natamux_core::js::js_trim;
use natamux_core::json::parse_js;
use serde_json::{Map, Value};

use super::{ModelUsage, UsageCounts, add_counts, zero_counts};

const UNKNOWN_MODEL: &str = "unknown";

type Json = Map<String, Value>;

fn object(value: Option<&Value>) -> Option<&Json> {
    value?.as_object()
}

// Numbers only, finite and above zero, floored.
fn count(value: Option<&Value>) -> u64 {
    match value.and_then(Value::as_f64) {
        Some(number) if number.is_finite() && number > 0.0 => number.floor() as u64,
        _ => 0,
    }
}

fn model_of(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str).map(js_trim) {
        Some(model) if !model.is_empty() => model.to_string(),
        _ => UNKNOWN_MODEL.to_string(),
    }
}

/// The JSON objects of a file, one per line. A broken line, such as the last one while the agent
/// still writes it, is skipped.
fn json_lines(path: &Path) -> Vec<Json> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut reader = BufReader::new(file);
    let mut out = Vec::new();
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&bytes);
        if let Ok(Value::Object(map)) = parse_js(line.trim_end_matches(['\n', '\r'])) {
            out.push(map);
        }
    }
    out
}

/// A message of Claude Code by id, in the order it first appeared.
#[derive(Default)]
struct ByMessage {
    index: HashMap<String, usize>,
    rows: Vec<(String, UsageCounts)>,
}

impl ByMessage {
    fn set(&mut self, id: String, model: String, counts: UsageCounts) {
        match self.index.get(&id) {
            Some(at) => self.rows[*at] = (model, counts),
            None => {
                self.index.insert(id, self.rows.len());
                self.rows.push((model, counts));
            }
        }
    }
}

// Claude Code writes one message on several lines, each with the usage so far: the last one counts.
fn read_claude_file(path: &Path, by_message: &mut ByMessage) {
    for (index, entry) in json_lines(path).iter().enumerate() {
        let message = object(entry.get("message"));
        let usage = object(message.and_then(|message| message.get("usage")));
        // "<synthetic>" marks a message that Claude Code made itself, without a model call.
        let (Some(message), Some(usage)) = (message, usage) else {
            continue;
        };
        if message.get("model").and_then(Value::as_str) == Some("<synthetic>") {
            continue;
        }
        let id = match message.get("id").and_then(Value::as_str) {
            Some(id) => id.to_string(),
            None => format!("{}:{}", path.display(), index + 1),
        };
        let long_cache = object(usage.get("cache_creation"));
        let counts = UsageCounts {
            input: count(usage.get("input_tokens")),
            cache_read: count(usage.get("cache_read_input_tokens")),
            cache_write: count(usage.get("cache_creation_input_tokens")),
            cache_write_1h: count(long_cache.and_then(|c| c.get("ephemeral_1h_input_tokens"))),
            output: count(usage.get("output_tokens")),
        };
        by_message.set(id, model_of(message.get("model")), counts);
    }
}

/// The session's file, plus its sub-agents in `<session>/subagents/*.jsonl` next to it.
pub fn read_claude_usage(path: &Path) -> ModelUsage {
    let mut by_message = ByMessage::default();
    read_claude_file(path, &mut by_message);
    let stem = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let stem = stem.unwrap_or_default();
    let stem = stem.strip_suffix(".jsonl").unwrap_or(&stem);
    let folder = path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(stem)
        .join("subagents");
    let mut files: Vec<_> = fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".jsonl"))
        .map(|entry| entry.path())
        .collect();
    files.sort();
    for file in files {
        read_claude_file(&file, &mut by_message);
    }
    let mut usage = ModelUsage::default();
    for (model, counts) in &by_message.rows {
        add_counts(&mut usage, model, counts);
    }
    usage
}

// Codex's input_tokens include the cached and cache-write tokens; output_tokens include reasoning.
fn codex_counts(total: &Json) -> UsageCounts {
    let cache_read = count(total.get("cached_input_tokens"));
    let cache_write = count(total.get("cache_write_input_tokens"));
    UsageCounts {
        input: count(total.get("input_tokens"))
            .saturating_sub(cache_read)
            .saturating_sub(cache_write),
        cache_read,
        cache_write,
        cache_write_1h: 0,
        output: count(total.get("output_tokens")),
    }
}

fn change(now: &UsageCounts, last: &UsageCounts) -> UsageCounts {
    UsageCounts {
        input: now.input.saturating_sub(last.input),
        cache_read: now.cache_read.saturating_sub(last.cache_read),
        cache_write: now.cache_write.saturating_sub(last.cache_write),
        cache_write_1h: now.cache_write_1h.saturating_sub(last.cache_write_1h),
        output: now.output.saturating_sub(last.output),
    }
}

fn is_empty(counts: &UsageCounts) -> bool {
    *counts == zero_counts()
}

/// Codex writes the session's running total after each model call, sometimes twice. Each change of
/// the total goes to the model of the latest turn.
pub fn read_codex_usage(path: &Path) -> ModelUsage {
    let mut usage = ModelUsage::default();
    let mut model = UNKNOWN_MODEL.to_string();
    let mut last = zero_counts();
    for entry in json_lines(path) {
        let payload = object(entry.get("payload"));
        let entry_type = entry.get("type").and_then(Value::as_str);
        let turn_model = payload
            .and_then(|payload| payload.get("model"))
            .filter(|name| name.is_string());
        if let (Some("turn_context"), Some(name)) = (entry_type, turn_model) {
            model = model_of(Some(name));
            continue;
        }
        let info = object(payload.and_then(|payload| payload.get("info")));
        let total = object(info.and_then(|info| info.get("total_token_usage")));
        let payload_type = payload
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str);
        let is_count = payload_type == Some("token_count");
        let Some(total) = total else { continue };
        if entry_type != Some("event_msg") || !is_count {
            continue;
        }
        let now = codex_counts(total);
        add_counts(&mut usage, &model, &change(&now, &last));
        last = now;
    }
    usage.0.retain(|(_, counts)| !is_empty(counts));
    usage
}

/// Copilot's inputTokens include cache reads and writes.
pub fn read_copilot_usage(path: &Path) -> ModelUsage {
    let mut usage = ModelUsage::default();
    for entry in json_lines(path) {
        let record = object(object(entry.get("data")).and_then(|data| data.get("usage")));
        let Some(record) = record else { continue };
        if entry.get("type").and_then(Value::as_str) != Some("session.usage_record") {
            continue;
        }
        let cache_read = count(record.get("cacheReadTokens"));
        let cache_write = count(record.get("cacheWriteTokens"));
        let counts = UsageCounts {
            input: count(record.get("inputTokens"))
                .saturating_sub(cache_read)
                .saturating_sub(cache_write),
            cache_read,
            cache_write,
            cache_write_1h: 0,
            output: count(record.get("outputTokens")),
        };
        add_counts(&mut usage, &model_of(record.get("model")), &counts);
    }
    usage
}

/// One JSON file. A message's `input` includes `cached`; its `thoughts` are output.
pub fn read_gemini_usage(path: &Path) -> ModelUsage {
    let mut usage = ModelUsage::default();
    let Ok(text) = fs::read(path) else {
        return usage;
    };
    let Ok(file) = parse_js(&String::from_utf8_lossy(&text)) else {
        return usage;
    };
    let messages = object(Some(&file)).and_then(|file| file.get("messages"));
    for message in messages.and_then(Value::as_array).into_iter().flatten() {
        let message = object(Some(message));
        let Some(tokens) = object(message.and_then(|message| message.get("tokens"))) else {
            continue;
        };
        let cache_read = count(tokens.get("cached"));
        let counts = UsageCounts {
            input: count(tokens.get("input")).saturating_sub(cache_read),
            cache_read,
            cache_write: 0,
            cache_write_1h: 0,
            output: count(tokens.get("output")) + count(tokens.get("thoughts")),
        };
        let model = model_of(message.and_then(|message| message.get("model")));
        add_counts(&mut usage, &model, &counts);
    }
    usage
}

#[cfg(test)]
#[path = "readers_tests.rs"]
mod tests;
