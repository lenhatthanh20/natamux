//! The hook files of `packages/daemon/src/agents/agent-launch.ts`: what each agent reads so it
//! reports to the daemon, and writing them into a workspace without touching the user's own.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use natamux_core::js::js_trim;
use natamux_core::json::{parse_js, to_js_string_pretty};
use natamux_proto::agents::claude_code::{CLAUDE_CODE_HOOK_EVENTS, TASK_ID_HEADER};
use serde_json::{Map, Value, json};

use crate::launch::{HOOK, LAUNCHABLE_AGENTS};

/// Codex takes hooks for one session as `-c` overrides (see `codex_hook_args`).
pub(crate) const CODEX_EVENTS: [&str; 6] = [
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "Interrupt",
];

/// The TOML value of one event's hooks.
pub(crate) fn codex_hook_value(event: &str) -> String {
    let tools = ["PreToolUse", "PostToolUse", "PermissionRequest"];
    let matcher = if tools.contains(&event) {
        "matcher=\"*\","
    } else {
        ""
    };
    format!("[{{{matcher}hooks=[{{type=\"command\",command=\"{HOOK} codex\",timeout=5}}]}}]")
}

/// The first line of the Codex project config Natamux writes; a file without it is the user's.
const CODEX_FILE_MARK: &str = "# Natamux hooks: Natamux writes this file again.";

/// The same hooks as `codex_hook_args`, for a Codex started by hand. Codex reads it only in a
/// folder it trusts. A Codex that Natamux starts gets them again with `-c`, which replaces each
/// key, so every hook runs once.
pub fn codex_hook_file() -> String {
    let mut lines = vec![CODEX_FILE_MARK.to_string(), "[hooks]".to_string()];
    lines.extend(
        CODEX_EVENTS
            .iter()
            .map(|event| format!("{event} = {}", codex_hook_value(event))),
    );
    format!("{}\n", lines.join("\n"))
}

fn events_object(events: &[&str], handler: impl Fn(&str) -> Value) -> Value {
    let map: Map<String, Value> = events
        .iter()
        .map(|event| ((*event).to_string(), handler(event)))
        .collect();
    Value::Object(map)
}

/// Gemini CLI reads these as its system settings when `GEMINI_CLI_SYSTEM_SETTINGS_PATH` names the
/// file, so the user's own settings stay as they are. Its timeouts are in milliseconds.
pub fn gemini_hook_settings() -> Value {
    let hook = json!({
        "type": "command", "name": "natamux", "command": format!("{HOOK} gemini"), "timeout": 5000,
    });
    let events = [
        "BeforeAgent",
        "BeforeTool",
        "AfterTool",
        "Notification",
        "AfterAgent",
    ];
    json!({
        // Inline, so its output goes into the terminal history. System settings win over the user's.
        "ui": { "useAlternateBuffer": false },
        "hooks": events_object(&events, |event| {
            if event.ends_with("Tool") {
                json!([{ "matcher": ".*", "hooks": [hook.clone()] }])
            } else {
                json!([{ "hooks": [hook.clone()] }])
            }
        }),
    })
}

/// Claude Code's hooks for a Claude Code started by hand. Commands, not the `http` hooks of
/// `--settings`: Copilot CLI reads this file too and refuses an `http` hook on loopback.
pub fn claude_code_hook_file() -> Value {
    let hook = json!({ "type": "command", "command": format!("{HOOK} claude-code"), "timeout": 5 });
    json!({
        "hooks": events_object(&CLAUDE_CODE_HOOK_EVENTS, |_| json!([{ "hooks": [hook.clone()] }])),
    })
}

/// Copilot CLI does not tell a hook which event it is, so each event names itself.
const COPILOT_EVENTS: [&str; 7] = [
    "userPromptSubmitted",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "notification",
    "agentStop",
    "errorOccurred",
];

pub fn copilot_hook_file() -> Value {
    json!({
        "version": 1,
        "hooks": events_object(&COPILOT_EVENTS, |event| {
            let command = format!("{HOOK} copilot {event}");
            json!([{ "type": "command", "bash": command, "powershell": command, "timeoutSec": 5 }])
        }),
    })
}

/// The Cursor CLI sends fewer events than the editor; the ones it does not send cost nothing.
const CURSOR_EVENTS: [&str; 8] = [
    "beforeSubmitPrompt",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "beforeShellExecution",
    "afterShellExecution",
    "afterFileEdit",
    "stop",
];

pub fn cursor_hook_file() -> Value {
    json!({
        "version": 1,
        "hooks": events_object(&CURSOR_EVENTS, |_| {
            json!([{ "command": format!("{HOOK} cursor"), "timeout": 5 }])
        }),
    })
}

/// A file an agent reads from the workspace, so an agent the user starts by hand reports too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub path: &'static str,
    pub content: String,
}

/// Whether to write Claude Code's file. With it in place a Claude Code that Natamux starts gets
/// no `--settings`, so only the daemon turns it on.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkspaceHooks {
    pub claude_code: bool,
}

/// `JSON.stringify(value, null, 2)` and a line break: the text of a hook file.
pub fn hook_file_text_of(value: &Value) -> String {
    format!("{}\n", to_js_string_pretty(value))
}

/// The hook file of an agent, if it has one for these options.
pub fn workspace_file(agent: &str, hooks: WorkspaceHooks) -> Option<WorkspaceFile> {
    // The launch of Claude Code and of Gemini leaves out its own copy when this file is in place
    // (see `launch_command`).
    let (path, content) = match agent {
        "claude-code" if hooks.claude_code => (
            ".claude/settings.local.json",
            hook_file_text_of(&claude_code_hook_file()),
        ),
        "gemini" => (
            ".gemini/settings.json",
            hook_file_text_of(&gemini_hook_settings()),
        ),
        "codex" => (".codex/config.toml", codex_hook_file()),
        "copilot" => (
            ".github/hooks/natamux.json",
            hook_file_text_of(&copilot_hook_file()),
        ),
        "cursor" => (".cursor/hooks.json", hook_file_text_of(&cursor_hook_file())),
        _ => return None,
    };
    Some(WorkspaceFile { path, content })
}

/// One hook handler of Natamux's: a command that runs `natamux hook`, or Claude Code's http hook,
/// which names the task in a header. Cursor names a command `command`; Copilot `bash` and
/// `powershell`.
pub fn is_natamux_hook(hook: &Value) -> bool {
    let Some(map) = hook.as_object() else {
        return false;
    };
    if let Some(url) = map.get("url") {
        return url.is_string()
            && map
                .get("headers")
                .and_then(Value::as_object)
                .is_some_and(|headers| headers.contains_key(TASK_ID_HEADER));
    }
    let commands: Vec<&Value> = ["command", "bash", "powershell"]
        .iter()
        .filter_map(|key| map.get(*key))
        .collect();
    !commands.is_empty()
        && commands
            .iter()
            .all(|each| each.as_str().is_some_and(|text| text.starts_with(HOOK)))
}

/// The JSON of a hook file that holds Natamux's hooks only, so writing them again loses no hook
/// of the user's. Other keys (Claude Code keeps the permissions the user allowed here) stay. None
/// for any other file. Claude Code and Gemini put the handlers of an event in `hooks`.
pub fn natamux_hook_json(text: &str) -> Option<Map<String, Value>> {
    let Value::Object(parsed) = parse_js(text).ok()? else {
        return None;
    };
    let Some(Value::Object(hooks)) = parsed.get("hooks") else {
        return None;
    };
    let mut handlers: Vec<&Value> = Vec::new();
    for value in hooks.values() {
        let entries: Vec<&Value> = match value {
            Value::Array(items) => items.iter().collect(),
            other => vec![other],
        };
        for entry in entries {
            match entry.get("hooks") {
                Some(Value::Array(inner)) if entry.is_object() => handlers.extend(inner),
                _ => handlers.push(entry),
            }
        }
    }
    (!handlers.is_empty() && handlers.iter().all(|hook| is_natamux_hook(hook))).then_some(parsed)
}

/// The text to write over `current`, or None to leave a file of the user's alone.
pub fn hook_file_text(file: &WorkspaceFile, current: &str) -> Option<String> {
    if current == file.content {
        return Some(current.to_string());
    }
    if file.path.ends_with(".toml") {
        return current
            .starts_with(CODEX_FILE_MARK)
            .then(|| file.content.clone());
    }
    let mut merged = natamux_hook_json(current)?;
    let Value::Object(new) = parse_js(&file.content).ok()? else {
        return None;
    };
    // A spread: keys of the file keep their place, the new values win.
    for (key, value) in new {
        merged.insert(key, value);
    }
    Some(hook_file_text_of(&Value::Object(merged)))
}

/// Writes the agent's hook file into the workspace and keeps it out of git. A file with a hook of
/// the user's own at that place is left alone: the agent then runs without Natamux's hooks, and
/// its terminal still shows that it works. Returns whether Natamux's hooks are in place.
pub fn prepare_workspace(agent: &str, workspace: &Path, hooks: WorkspaceHooks) -> bool {
    let Some(file) = workspace_file(agent, hooks) else {
        return false;
    };
    let target = workspace.join(file.path);
    let text = if target.exists() {
        match fs::read(&target) {
            Ok(bytes) => hook_file_text(&file, &String::from_utf8_lossy(&bytes)),
            Err(_) => return false,
        }
    } else {
        Some(file.content.clone())
    };
    let Some(text) = text else {
        return false;
    };
    // A folder that cannot be written: the agent still starts, followed by its terminal only.
    let written = target
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(&target, text));
    if written.is_err() {
        return false;
    }
    exclude_from_git(workspace, &format!("/{}", file.path));
    true
}

/// The hook files of all agents, so an agent the user starts by hand in any shell reports to the
/// daemon too.
pub fn prepare_agent_hooks(workspace: &Path, hooks: WorkspaceHooks) {
    for agent in LAUNCHABLE_AGENTS {
        prepare_workspace(agent, workspace, hooks);
    }
}

/// Adds a line to the repository's own ignore list, which all its worktrees share. Not a git
/// worktree, or any failure: there is nothing to do.
pub fn exclude_from_git(worktree: &Path, line: &str) {
    let _ = try_exclude(worktree, line);
}

fn try_exclude(worktree: &Path, line: &str) -> std::io::Result<()> {
    if let Some(common) = crate::git_files::common_dir(worktree) {
        return add_exclude_line(worktree, &common, line);
    }
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    add_exclude_line(worktree, Path::new(js_trim(&stdout)), line)
}

/// `common` is what `git rev-parse --git-common-dir` printed: absolute, or from the worktree.
fn add_exclude_line(worktree: &Path, common: &Path, line: &str) -> std::io::Result<()> {
    let info = if common.is_absolute() {
        common.join("info")
    } else {
        worktree.join(common).join("info")
    };
    let exclude = info.join("exclude");
    let text = if exclude.exists() {
        String::from_utf8_lossy(&fs::read(&exclude)?).into_owned()
    } else {
        String::new()
    };
    if text
        .split('\n')
        .any(|each| each.strip_suffix('\r').unwrap_or(each) == line)
    {
        return Ok(());
    }
    fs::create_dir_all(&info)?;
    let lead = if text.is_empty() || text.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&exclude)?;
    file.write_all(format!("{lead}{line}\n").as_bytes())
}

#[cfg(test)]
#[path = "hook_files_tests.rs"]
mod tests;
