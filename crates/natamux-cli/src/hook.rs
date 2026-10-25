//! `natamux hook <agent> [event]` (`packages/cli/src/forward-hook.ts`): sends the JSON of an
//! agent's hook to the daemon. It runs inside the agent, so it must never make the agent fail or
//! print anything: it always exits 0, and any problem ends silently (NATAMUX_DEBUG shows it).
//! Without NATAMUX_TOKEN the session is not one of Natamux's, so it does nothing at all.
//! It never reads the proxy variables, so a proxy on the machine cannot block it.

use std::time::Duration;

use natamux_client::Method;
use natamux_proto::agents::claude_code::{TASK_ID_HEADER, TERMINAL_HEADER};
use natamux_proto::terminals::query::encode;

use crate::daemon::Transport;
use crate::format::DEFAULT_URL;
use crate::run::{Env, Io};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
/// Agents that read a hook's output as JSON (Cursor blocks a shell command on an answer that is
/// not JSON).
const JSON_ANSWER_AGENTS: [&str; 2] = ["cursor", "gemini"];

/// `^[a-z][a-z0-9-]*$`
fn is_agent_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// `^[A-Za-z][A-Za-z0-9_-]{0,63}$`
fn is_event_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 64
        && bytes
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// The arguments after "hook". Always 0: an agent treats exit code 2 as "block this action".
pub async fn run_hook_command<T: Transport>(
    args: &[String],
    env: &Env,
    io: &mut dyn Io,
    read_stdin: &dyn Fn() -> Vec<u8>,
    connect: &dyn Fn(&str, &str) -> T,
) -> u8 {
    let agent = args.first().map(String::as_str);
    let event = args.get(1).map(String::as_str);
    let extra = args.get(2..).unwrap_or_default();
    let problem = match agent {
        Some(agent) if is_agent_name(agent) => {
            if !extra.is_empty() {
                Some(format!("Unexpected argument: {}", extra.join(" ")))
            } else {
                event
                    .filter(|event| !is_event_name(event))
                    .map(|event| format!("Unexpected argument: {event}"))
            }
        }
        _ => Some(
            "An agent name is required, in lower case letters, digits and \"-\": natamux hook claude-code"
                .into(),
        ),
    };
    let (Some(agent), None) = (agent, problem.as_ref()) else {
        io.stderr(&format!(
            "{}\nRun \"natamux help\" for usage.\n",
            problem.unwrap_or_default()
        ));
        return 0;
    };

    // Copilot CLI also runs the hooks in `.claude/settings.local.json`; only Claude Code sets
    // CLAUDECODE, so Copilot's events never reach the daemon as Claude Code's.
    if agent == "claude-code" && env.get("CLAUDECODE") != Some("1") {
        return 0;
    }
    if JSON_ANSWER_AGENTS.contains(&agent) {
        io.stdout("{}\n");
    }
    // Copilot CLI does not say which event a hook is for, so its hook command names it.
    if let Err(reason) = forward(agent, event, env, read_stdin, connect).await
        && env
            .get("NATAMUX_DEBUG")
            .is_some_and(|value| !value.is_empty())
    {
        io.stderr(&format!("natamux hook {agent}: {reason}\n"));
    }
    0
}

async fn forward<T: Transport>(
    agent: &str,
    event: Option<&str>,
    env: &Env,
    read_stdin: &dyn Fn() -> Vec<u8>,
    connect: &dyn Fn(&str, &str) -> T,
) -> Result<(), String> {
    let Some(token) = env.trimmed("NATAMUX_TOKEN") else {
        return Ok(());
    };
    let mut body = read_stdin();
    // Cursor on Windows starts its JSON with a byte order mark, which JSON parsers refuse.
    if body.starts_with(b"\xEF\xBB\xBF") {
        body.drain(..3);
    }
    if String::from_utf8_lossy(&body).trim().is_empty() {
        return Ok(());
    }
    let mut headers = vec![("content-type", "application/json")];
    if let Some(task) = env.trimmed("NATAMUX_TASK_ID") {
        headers.push((TASK_ID_HEADER, task));
    }
    if let Some(terminal) = env.trimmed("NATAMUX_TERMINAL") {
        headers.push((TERMINAL_HEADER, terminal));
    }
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
    let query = event
        .map(|event| format!("?event={}", encode(event)))
        .unwrap_or_default();
    let path = format!("/hooks/{}{query}", encode(agent));
    let transport = connect(base_url, token);
    let sent = tokio::time::timeout(
        REQUEST_TIMEOUT,
        transport.request(Method::POST, &path, &headers, Some(body)),
    )
    .await
    .map_err(|_| "The operation was aborted due to timeout".to_string())??;
    match sent {
        (status, _) if (200..300).contains(&status) => Ok(()),
        (status, _) => Err(format!("the daemon answered {status}")),
    }
}
