//! The PATH of the user's login shell, sent by the desktop app once the daemon is ready
//! (`login-path.ts`). Until it comes the daemon has the app's own PATH, which may miss Homebrew,
//! nvm and the like. The Rust daemon has no desktop parent yet (phase 5 picks the channel), so
//! `main` passes none and nothing waits.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use natamux_pty::Env;
use serde_json::Value;

/// Set by the desktop app when it will send the PATH.
const PENDING: &str = "NATAMUX_PATH_PENDING";
/// Longer than the app's 10 s limit for the shell. A PATH that does not come is a bug; the daemon
/// then keeps the PATH it has.
pub const WAIT: Duration = Duration::from_secs(15);

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
    let deadline = Instant::now() + wait;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match port.recv_timeout(left) {
            Ok(message) => {
                if let Some(path) = message.get("path").and_then(Value::as_str) {
                    env.insert("PATH".into(), path.into());
                    return Some(path.to_string());
                }
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

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
        send.send(json!({ "other": 1 })).unwrap();
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
}
