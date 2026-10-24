//! The PATH of the user's login shell, sent by the desktop app before the daemon starts services.
//! If it does not arrive, the daemon keeps the app's PATH. The app sends one message over the
//! child's private stdin.

use std::io::{BufRead, Read};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use natamux_pty::Env;
use serde_json::Value;
use tokio::sync::oneshot;

/// Set by the desktop app when it will send the PATH.
const PENDING: &str = "NATAMUX_PATH_PENDING";
/// Longer than the app's 10 s limit for the shell. A PATH that does not come is a bug; the daemon
/// then keeps the PATH it has.
pub const WAIT: Duration = Duration::from_secs(15);
const MAX_PATH_BYTES: usize = 4096;
const MAX_MESSAGE_BYTES: usize = 8192;

/// The child's stdin is private to its parent. Read one line and leave the bounded wait to the
/// receiver, so a parent that dies or never writes cannot hold daemon startup forever.
pub fn read_path_message(input: impl BufRead, send: Sender<Value>, parent_end: oneshot::Sender<()>) {
    let mut limited = input.take(MAX_MESSAGE_BYTES as u64 + 1);
    let mut line = String::new();
    if limited.read_line(&mut line).is_ok()
        && line.len() <= MAX_MESSAGE_BYTES
        && line.ends_with('\n')
        && let Ok(message) = serde_json::from_str(&line)
    {
        let _ = send.send(message);
    }
    let mut input = limited.into_inner();
    let _ = std::io::copy(&mut input, &mut std::io::sink());
    let _ = parent_end.send(());
}

/// Sets the login PATH in `env` when it comes and gives it. None at once when there is none to
/// wait for, and None when it does not come in time.
pub fn wait_for_login_path(
    env: &mut Env,
    port: Option<&Receiver<Value>>,
    wait: Duration,
) -> Option<String> {
    let pending = env.get(PENDING).map(String::as_str) == Some("1");
    // The shells and agents must not see it.
    env.remove(PENDING);
    let port = port.filter(|_| pending)?;
    let message = match port.recv_timeout(wait) {
        Ok(message) => message,
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
    };
    let path = message.get("path").and_then(Value::as_str)?;
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.chars().any(|part| matches!(part, '\0' | '\n' | '\r'))
    {
        return None;
    }
    env.insert("PATH".into(), path.into());
    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::time::Instant;

    use serde_json::json;

    use super::*;

    fn pending() -> Env {
        [("NATAMUX_PATH_PENDING", "1"), ("PATH", "/usr/bin")]
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn is_waited_for_then_set_for_the_programs_the_daemon_runs() {
        let mut env = pending();
        let (send, port) = channel();
        send.send(json!({ "path": "/opt/homebrew/bin:/usr/bin" }))
            .unwrap();
        let path = wait_for_login_path(&mut env, Some(&port), WAIT);
        assert_eq!(path.as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(env["PATH"], "/opt/homebrew/bin:/usr/bin");
        assert!(!env.contains_key("NATAMUX_PATH_PENDING"));
    }

    #[test]
    fn keeps_the_path_it_has_when_none_comes_in_time_and_ignores_a_late_one() {
        let mut env = pending();
        let (send, port) = channel();
        assert_eq!(
            wait_for_login_path(&mut env, Some(&port), Duration::from_millis(20)),
            None
        );
        send.send(json!({ "path": "/late" })).unwrap();
        assert_eq!(env["PATH"], "/usr/bin");
    }

    #[test]
    fn is_not_waited_for_outside_the_desktop_app_or_when_the_app_sends_none() {
        let mut env = pending();
        assert_eq!(wait_for_login_path(&mut env, None, WAIT), None);
        assert!(!env.contains_key("NATAMUX_PATH_PENDING"));
        let mut plain: Env = Env::new();
        let (_send, port) = channel();
        let started = Instant::now();
        assert_eq!(wait_for_login_path(&mut plain, Some(&port), WAIT), None);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn reads_one_path_message_from_the_private_pipe() {
        let (send, receive) = channel();
        let (parent_end, mut ended) = oneshot::channel();
        read_path_message(std::io::Cursor::new(b"{\"path\":\"/bin\"}\nignored\n"), send, parent_end);
        assert_eq!(receive.try_recv().unwrap(), json!({ "path": "/bin" }));
        assert!(receive.try_recv().is_err());
        assert!(ended.try_recv().is_ok());
    }

    #[test]
    fn rejects_a_large_or_multiline_path_and_keeps_the_fallback() {
        for path in ["x".repeat(MAX_PATH_BYTES + 1), "/bin\n/evil".into()] {
            let mut env = pending();
            let (send, port) = channel();
            send.send(json!({ "path": path })).unwrap();
            assert_eq!(wait_for_login_path(&mut env, Some(&port), WAIT), None);
            assert_eq!(env["PATH"], "/usr/bin");
        }
    }

    #[test]
    fn rejects_a_large_private_pipe_message() {
        let (send, receive) = channel();
        let (parent_end, mut ended) = oneshot::channel();
        let line = format!("{{\"path\":\"{}\"}}\n", "x".repeat(MAX_MESSAGE_BYTES));
        read_path_message(std::io::Cursor::new(line), send, parent_end);
        assert!(receive.try_recv().is_err());
        assert!(ended.try_recv().is_ok());
    }
}
