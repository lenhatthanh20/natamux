//! What an agent tells Natamux (`agent-signals.ts`, `summary-text.ts`, `agent-adapters.ts`): the
//! signals, one adapter per agent that turns its hook into the common form, and the events a
//! signal means for a task in a state.

use std::path::Path;

use natamux_core::js::{is_js_space, utf16_len};
use natamux_core::zod::ZodError;
use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, Value};

pub mod claude_code;
pub mod codex;
pub mod copilot;
pub mod cursor;
mod fields;
pub mod gemini;

pub use copilot::COPILOT_EVENT_FIELD;

pub const SUMMARY_LENGTH: usize = 80;

/// What an agent tells Natamux, whatever the agent calls it. `TurnPaused`: the turn ended, but the
/// agent's background work will wake it again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Prompt,
    Activity,
    Asking,
    TurnEnded,
    TurnPaused,
    Failed,
}

/// `AGENT_SIGNALS`.
pub const SIGNALS: [Signal; 6] = [
    Signal::Prompt,
    Signal::Activity,
    Signal::Asking,
    Signal::TurnEnded,
    Signal::TurnPaused,
    Signal::Failed,
];

impl Signal {
    pub fn as_str(self) -> &'static str {
        match self {
            Signal::Prompt => "prompt",
            Signal::Activity => "activity",
            Signal::Asking => "asking",
            Signal::TurnEnded => "turn-ended",
            Signal::TurnPaused => "turn-paused",
            Signal::Failed => "failed",
        }
    }
}

/// A hook of one agent, turned into the common form.
#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedHook {
    pub signal: Signal,
    pub cwd: Option<String>,
    /// In the key order Node builds.
    pub details: Map<String, Value>,
    /// One line on what the agent does now, such as "Edit src/a.ts".
    pub summary: Option<String>,
    /// The session file that holds the token usage. Not in `details`, so it stays out of the log.
    pub transcript: Option<String>,
}

pub type HookResult = Result<Option<NormalizedHook>, ZodError>;

/// One adapter per agent. `normalize` gives Err for input that is not a hook of this agent, and
/// Ok(None) for a hook that Natamux does not use.
#[derive(Clone, Copy)]
pub struct Adapter {
    id: &'static str,
    run: fn(&Value, &Path) -> HookResult,
}

impl Adapter {
    pub fn id(&self) -> &'static str {
        self.id
    }

    /// `home` is where Copilot keeps its sessions.
    pub fn normalize(&self, raw: &Value, home: &Path) -> HookResult {
        (self.run)(raw, home)
    }
}

// Add an agent here: write its adapter, then list it.
const ADAPTERS: [Adapter; 5] = [
    Adapter {
        id: "claude-code",
        run: claude_code::normalize,
    },
    Adapter {
        id: "codex",
        run: codex::normalize,
    },
    Adapter {
        id: "gemini",
        run: gemini::normalize,
    },
    Adapter {
        id: "copilot",
        run: copilot::normalize,
    },
    Adapter {
        id: "cursor",
        run: cursor::normalize,
    },
];

pub fn find_adapter(id: &str) -> Option<Adapter> {
    ADAPTERS.iter().find(|adapter| adapter.id == id).copied()
}

pub fn known_agent_ids() -> Vec<&'static str> {
    ADAPTERS.iter().map(|adapter| adapter.id).collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedEvent {
    pub kind: &'static str,
    pub payload: Map<String, Value>,
}

fn planned(kind: &'static str, payload: &Map<String, Value>) -> PlannedEvent {
    PlannedEvent {
        kind,
        payload: payload.clone(),
    }
}

// Brings a task that waits for the user back to running, then adds the closing event.
fn end_turn(
    state: TaskState,
    closing: &'static str,
    payload: &Map<String, Value>,
) -> Vec<PlannedEvent> {
    match state {
        TaskState::Running => vec![planned(closing, payload)],
        TaskState::NeedsInput => vec![planned("agent.resumed", payload), planned(closing, payload)],
        _ => Vec::new(),
    }
}

/// Which task events a signal means for a task in this state. An empty list means "ignore it", so
/// the event log does not fill up with calls that change nothing.
pub fn plan_signal_events(
    state: TaskState,
    signal: Signal,
    payload: &Map<String, Value>,
) -> Vec<PlannedEvent> {
    match signal {
        Signal::Prompt => match state {
            TaskState::Draft => vec![
                planned("task.readied", payload),
                planned("agent.started", payload),
            ],
            TaskState::Ready => vec![planned("agent.started", payload)],
            TaskState::NeedsInput | TaskState::Review | TaskState::Failed => {
                vec![planned("agent.resumed", payload)]
            }
            _ => Vec::new(),
        },
        // A tool ran, so a permission that was asked for has been answered, or the agent works
        // again after an API error.
        Signal::Activity => match state {
            TaskState::NeedsInput | TaskState::Failed => vec![planned("agent.resumed", payload)],
            _ => Vec::new(),
        },
        Signal::Asking if state == TaskState::Running => vec![planned("agent.asked", payload)],
        Signal::Asking => Vec::new(),
        Signal::TurnEnded => end_turn(state, "agent.finished", payload),
        Signal::TurnPaused => end_turn(state, "agent.waiting", payload),
        Signal::Failed => end_turn(state, "agent.failed", payload),
    }
}

/// `text.slice(0, units)`, counted in UTF-16 units. A pair cut in the middle is dropped whole,
/// where JS keeps a lone surrogate.
pub(crate) fn slice_utf16(text: &str, units: usize) -> &str {
    let mut used = 0;
    let mut end = 0;
    for (index, c) in text.char_indices() {
        used += c.len_utf16();
        if used > units {
            break;
        }
        end = index + c.len_utf8();
    }
    &text[..end]
}

/// One line: white space collapsed, cut to `length` UTF-16 units with an ellipsis.
pub fn shorten(text: &str, length: usize) -> String {
    let mut line = String::new();
    let mut gap = false;
    for c in text.chars() {
        if is_js_space(c) {
            gap = true;
            continue;
        }
        if gap && !line.is_empty() {
            line.push(' ');
        }
        gap = false;
        line.push(c);
    }
    if utf16_len(&line) > length {
        format!("{}…", slice_utf16(&line, length.saturating_sub(1)))
    } else {
        line
    }
}

// A path inside the session's folder is shown from there.
fn relative_path<'a>(path: &'a str, cwd: Option<&str>) -> &'a str {
    match cwd {
        Some(cwd) if !cwd.is_empty() => path
            .strip_prefix(cwd)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or(path),
        _ => path,
    }
}

/// "Edit src/a.ts", "Bash: pnpm test", or the tool name for any other tool. The other adapters
/// turn their hooks into Claude Code's shape before they call it.
pub fn tool_summary(
    tool_name: Option<&str>,
    tool_input: Option<&Map<String, Value>>,
    cwd: Option<&str>,
) -> Option<String> {
    let tool = tool_name.filter(|name| !name.is_empty())?;
    let text = |key: &str| {
        tool_input
            .and_then(|input| input.get(key))
            .and_then(Value::as_str)
    };
    let file = text("file_path")
        .or_else(|| text("notebook_path"))
        .or_else(|| text("absolute_path"))
        .or_else(|| text("path"))
        .filter(|file| !file.is_empty());
    if let Some(file) = file {
        let line = format!("{tool} {}", relative_path(file, cwd));
        return Some(shorten(&line, SUMMARY_LENGTH));
    }
    if let Some(command) = text("command").filter(|command| !command.is_empty()) {
        return Some(shorten(&format!("{tool}: {command}"), SUMMARY_LENGTH));
    }
    let pattern = text("pattern")
        .or_else(|| text("url"))
        .or_else(|| text("query"))
        .or_else(|| text("description"))
        .filter(|pattern| !pattern.is_empty());
    let line = match pattern {
        Some(pattern) => format!("{tool} {pattern}"),
        None => tool.to_string(),
    };
    Some(shorten(&line, SUMMARY_LENGTH))
}

#[cfg(test)]
#[path = "hooks_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "hooks/adapters_tests.rs"]
mod adapters_tests;
