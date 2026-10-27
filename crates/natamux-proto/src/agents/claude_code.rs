//! How Claude Code reports to the daemon (`packages/core/src/agents/claude-code-hooks.ts`).

/// The hook events Natamux listens to, in core's order.
pub const CLAUDE_CODE_HOOK_EVENTS: [&str; 7] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Notification",
    "Stop",
    "StopFailure",
];

/// The header that names the task of a hook.
pub const TASK_ID_HEADER: &str = "X-Natamux-Task";
/// The terminal the agent runs in, which tells the stage inside a group.
pub const TERMINAL_HEADER: &str = "X-Natamux-Terminal";

pub const TOKEN_ENV_VAR: &str = "NATAMUX_TOKEN";
pub const TASK_ENV_VAR: &str = "NATAMUX_TASK_ID";
pub const TERMINAL_ENV_VAR: &str = "NATAMUX_TERMINAL";

/// Claude Code tells its plan limits only to its status line, so Natamux's status line
/// (`natamux statusline`) passes them on and then runs the user's own.
pub const STATUS_LINE_ARG: &str = "statusline";

/// Whether a `statusLine` of Claude Code's settings is Natamux's own.
pub fn is_natamux_status_line(line: &serde_json::Value) -> bool {
    line.get("command")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|command| {
            command.contains("natamux") && command.trim_end().ends_with(STATUS_LINE_ARG)
        })
}
