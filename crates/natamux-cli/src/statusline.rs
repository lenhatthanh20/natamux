//! `natamux statusline`: the status line command of a Claude Code that Natamux starts. Claude Code
//! gives it JSON on stdin with the plan's `rate_limits`; it sends them to the daemon while it runs
//! the user's own status line command with the same input, and prints what that prints. Like the
//! hook, it never fails Claude Code: it always exits 0 and ends silently (NATAMUX_DEBUG shows why).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use natamux_client::Method;
use natamux_proto::agents::claude_code::is_natamux_status_line;
use serde_json::Value;

use crate::daemon::Transport;
use crate::format::DEFAULT_URL;
use crate::run::{Env, Io};

const REQUEST_TIMEOUT: Duration = Duration::from_millis(800);
/// Set on the user's command, so a command that runs `natamux statusline` again ends at once.
const NESTED_ENV: &str = "NATAMUX_STATUSLINE";

/// The `rate_limits` object of the input, when it has one.
pub fn rate_limits_of(input: &Value) -> Option<Value> {
    input
        .get("rate_limits")
        .filter(|limits| limits.is_object())
        .cloned()
}

/// The status line command of one settings file, unless it is Natamux's own.
pub fn own_command(settings: &str) -> Option<String> {
    let value: Value = serde_json::from_str(settings).ok()?;
    let line = value.get("statusLine")?;
    if line.get("type").and_then(Value::as_str) != Some("command") || is_natamux_status_line(line) {
        return None;
    }
    let command = line.get("command")?.as_str()?.trim();
    (!command.is_empty()).then(|| command.to_string())
}

/// The files Claude Code reads a status line from after Natamux's own, in its order: the project's
/// local and shared settings, then the user's.
fn settings_files(input: &Value, env: &Env) -> Vec<PathBuf> {
    let project = input
        .pointer("/workspace/project_dir")
        .or_else(|| input.get("cwd"))
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let user = match env.trimmed("CLAUDE_CONFIG_DIR") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => env
            .trimmed("HOME")
            .map(|home| Path::new(home).join(".claude")),
    };
    let mut files = Vec::new();
    if let Some(project) = project {
        files.push(project.join(".claude").join("settings.local.json"));
        files.push(project.join(".claude").join("settings.json"));
    }
    if let Some(user) = user {
        files.push(user.join("settings.json"));
    }
    files
}

fn shell(command: &str) -> Command {
    if cfg!(windows) {
        let mut shell = Command::new("cmd");
        shell.arg("/C");
        #[cfg(windows)]
        std::os::windows::process::CommandExt::raw_arg(&mut shell, command);
        shell
    } else {
        let mut shell = Command::new("sh");
        shell.args(["-c", command]);
        shell
    }
}

/// Starts the user's command with the input on its stdin.
fn start_own(command: &str, input: &[u8]) -> Option<Child> {
    let mut child = shell(command)
        .env(NESTED_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    Some(child)
}

pub async fn run_statusline<T: Transport>(
    env: &Env,
    io: &mut dyn Io,
    read_stdin: &dyn Fn() -> Vec<u8>,
    connect: &dyn Fn(&str, &str) -> T,
) -> u8 {
    if env.get(NESTED_ENV).is_some() {
        return 0;
    }
    let input = read_stdin();
    let parsed: Value = serde_json::from_slice(&input).unwrap_or(Value::Null);
    let own = settings_files(&parsed, env)
        .iter()
        .find_map(|path| own_command(&std::fs::read_to_string(path).ok()?));
    let child = own.and_then(|command| start_own(&command, &input));
    if let Err(reason) = send(env, &parsed, connect).await
        && env
            .get("NATAMUX_DEBUG")
            .is_some_and(|value| !value.is_empty())
    {
        io.stderr(&format!("natamux statusline: {reason}\n"));
    }
    if let Some(output) = child.and_then(|child| child.wait_with_output().ok()) {
        io.stdout(&String::from_utf8_lossy(&output.stdout));
    }
    0
}

async fn send<T: Transport>(
    env: &Env,
    input: &Value,
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
    use serde_json::json;

    #[test]
    fn reads_the_rate_limits_of_the_input() {
        let input =
            json!({"model": {"id": "m"}, "rate_limits": {"five_hour": {"used_percentage": 12}}});
        assert_eq!(
            rate_limits_of(&input),
            Some(json!({"five_hour": {"used_percentage": 12}}))
        );
        assert_eq!(rate_limits_of(&json!({"model": {}})), None);
        assert_eq!(rate_limits_of(&Value::Null), None);
    }

    #[test]
    fn finds_the_users_own_command_but_not_natamuxs() {
        let own = r#"{"statusLine":{"type":"command","command":" ~/.claude/line.sh "}}"#;
        assert_eq!(own_command(own).as_deref(), Some("~/.claude/line.sh"));
        for ours in [
            r#"{"statusLine":{"type":"command","command":"natamux statusline"}}"#,
            r#"{"statusLine":{"type":"command","command":"\"/Apps/MacOS/natamux\" statusline"}}"#,
        ] {
            assert_eq!(own_command(ours), None, "{ours}");
        }
        assert_eq!(own_command(r#"{"statusLine":{"type":"static"}}"#), None);
        assert_eq!(own_command("{}"), None);
    }

    #[test]
    fn looks_in_the_project_then_the_user_settings() {
        let input = json!({"cwd": "/w/sub", "workspace": {"project_dir": "/w"}});
        let env = Env::from([("HOME", "/home/me")]);
        let files: Vec<String> = settings_files(&input, &env)
            .iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(
            files,
            [
                "/w/.claude/settings.local.json",
                "/w/.claude/settings.json",
                "/home/me/.claude/settings.json",
            ]
        );
    }
}
