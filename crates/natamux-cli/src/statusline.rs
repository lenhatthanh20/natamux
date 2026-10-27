//! `natamux statusline`: the status line command of a Claude Code that Natamux starts. Claude Code
//! gives it JSON on stdin with the plan's `rate_limits`; it sends them to the daemon, then runs the
//! user's own status line command with the same input and prints what that prints. Like the hook,
//! it never fails Claude Code: it always exits 0 and ends silently (NATAMUX_DEBUG shows why).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use natamux_client::Method;
use serde_json::Value;

use crate::daemon::Transport;
use crate::format::DEFAULT_URL;
use crate::run::{Env, Io};

const REQUEST_TIMEOUT: Duration = Duration::from_millis(800);
const THIS_COMMAND: &str = "natamux statusline";

/// The `rate_limits` object of the input, when it has one.
pub fn rate_limits_of(input: &[u8]) -> Option<Value> {
    let value: Value = serde_json::from_slice(input).ok()?;
    value
        .get("rate_limits")
        .filter(|limits| limits.is_object())
        .cloned()
}

/// The user's own status line command in their Claude settings. None when there is none, or it
/// is this command.
pub fn own_command(settings: &str) -> Option<String> {
    let value: Value = serde_json::from_str(settings).ok()?;
    let line = value.get("statusLine")?;
    if line.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }
    let command = line.get("command")?.as_str()?.trim();
    (!command.is_empty() && !command.starts_with(THIS_COMMAND)).then(|| command.to_string())
}

// shortcut: only the user's settings file; a project's own `statusLine` is not run. Add the
// project files when someone asks for them.
fn user_settings(env: &Env) -> Option<PathBuf> {
    let dir = match env.trimmed("CLAUDE_CONFIG_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(env.trimmed("HOME")?).join(".claude"),
    };
    Some(dir.join("settings.json"))
}

/// Runs the user's command with the input, as Claude Code would: its first line of output is the
/// status line.
fn run_own(command: &str, input: &[u8]) -> Option<String> {
    let mut child = if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
    } else {
        Command::new("sh")
            .args(["-c", command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
    }
    .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    let output = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub async fn run_statusline<T: Transport>(
    env: &Env,
    io: &mut dyn Io,
    read_stdin: &dyn Fn() -> Vec<u8>,
    connect: &dyn Fn(&str, &str) -> T,
) -> u8 {
    let input = read_stdin();
    if let Err(reason) = send(env, &input, connect).await
        && env
            .get("NATAMUX_DEBUG")
            .is_some_and(|value| !value.is_empty())
    {
        io.stderr(&format!("natamux statusline: {reason}\n"));
    }
    let own = user_settings(env)
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| own_command(&text));
    if let Some(text) = own.and_then(|command| run_own(&command, &input)) {
        io.stdout(&text);
    }
    0
}

async fn send<T: Transport>(
    env: &Env,
    input: &[u8],
    connect: &dyn Fn(&str, &str) -> T,
) -> Result<(), String> {
    let (Some(token), Some(limits)) = (env.trimmed("NATAMUX_TOKEN"), rate_limits_of(input)) else {
        return Ok(());
    };
    let base_url = env
        .trimmed("NATAMUX_URL")
        .unwrap_or(DEFAULT_URL)
        .trim_end_matches('/');
    // shortcut: plain HTTP only, as the rest of the command (src/run.rs).
    if !base_url.starts_with("http://") {
        return Err(format!(
            "NATAMUX_URL must start with http:// (got {base_url:?})"
        ));
    }
    let body = serde_json::to_vec(&limits).map_err(|error| error.to_string())?;
    let headers = [("content-type", "application/json")];
    let transport = connect(base_url, token);
    let sent = tokio::time::timeout(
        REQUEST_TIMEOUT,
        transport.request(Method::POST, "/limits/claude-code", &headers, Some(body)),
    )
    .await
    .map_err(|_| "The operation was aborted due to timeout".to_string())??;
    match sent {
        (status, _) if (200..300).contains(&status) => Ok(()),
        (status, _) => Err(format!("the daemon answered {status}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_rate_limits_of_the_input() {
        let input = br#"{"model":{"id":"m"},"rate_limits":{"five_hour":{"used_percentage":12}}}"#;
        assert_eq!(
            rate_limits_of(input),
            Some(serde_json::json!({"five_hour": {"used_percentage": 12}}))
        );
        assert_eq!(rate_limits_of(br#"{"model":{}}"#), None);
        assert_eq!(rate_limits_of(b"not json"), None);
    }

    #[test]
    fn finds_the_users_own_command_but_not_this_one() {
        let own = r#"{"statusLine":{"type":"command","command":" ~/.claude/line.sh "}}"#;
        assert_eq!(own_command(own).as_deref(), Some("~/.claude/line.sh"));
        let this = r#"{"statusLine":{"type":"command","command":"natamux statusline"}}"#;
        assert_eq!(own_command(this), None);
        assert_eq!(own_command(r#"{"statusLine":{"type":"static"}}"#), None);
        assert_eq!(own_command("{}"), None);
    }
}
