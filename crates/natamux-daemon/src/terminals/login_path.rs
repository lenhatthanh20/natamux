//! The PATH of the user's login shell, sent by the desktop app after the daemon started. The
//! daemon does not wait for it: `natamux_core::login_path` holds it, and a spawn waits there. If it
//! does not arrive, the daemon keeps the app's PATH. The app sends one message over the child's
//! private stdin.

use std::io::BufRead;
use std::time::Duration;

use natamux_core::login_path::{self, LoginPath};
use natamux_pty::Env;
use serde_json::Value;
use tokio::sync::oneshot;

/// Set by the desktop app when it will send the PATH.
pub const PENDING: &str = "NATAMUX_PATH_PENDING";
/// Longer than the app's 10 s limit for the shell. A PATH that does not come is a bug; the daemon
/// then keeps the PATH it has.
pub const WAIT: Duration = Duration::from_secs(15);
const MAX_PATH_BYTES: usize = 4096;
const MAX_MESSAGE_BYTES: usize = 8192;

/// Marks the PATH as coming and takes the variable out of `env`: shells and agents must not see it.
/// True when the app will send the PATH.
pub fn expect_login_path(env: &mut Env) -> bool {
    let pending = env.remove(PENDING).as_deref() == Some("1");
    if pending {
        login_path::expect(WAIT);
    }
    pending
}

/// Gives a shell the login PATH, with the app's `bin_dir` first as `shell_env` puts it. Waits for
/// the PATH; no login PATH leaves the env as it is.
pub fn use_login_path(env: &mut Env, bin_dir: Option<&str>) {
    if let Some(login) = login_path::wait() {
        env.insert("PATH".into(), path_with_bin_dir(bin_dir, &login));
    }
}

fn path_with_bin_dir(bin_dir: Option<&str>, login: &str) -> String {
    let separator = if cfg!(windows) { ';' } else { ':' };
    match bin_dir {
        Some(dir) => format!("{dir}{separator}{login}"),
        None => login.to_string(),
    }
}

/// The child's stdin is private to its parent. Read one line; a bad or missing one, or a parent
/// that dies, leaves the inherited PATH.
pub fn read_path_message(input: impl BufRead, parent_end: oneshot::Sender<()>) {
    read_into_cell(&login_path::LOGIN_PATH, input, parent_end);
}

fn read_into_cell(cell: &LoginPath, input: impl BufRead, parent_end: oneshot::Sender<()>) {
    let mut limited = input.take(MAX_MESSAGE_BYTES as u64 + 1);
    let mut line = String::new();
    let path = if limited.read_line(&mut line).is_ok()
        && line.len() <= MAX_MESSAGE_BYTES
        && line.ends_with('\n')
        && let Ok(message) = serde_json::from_str::<Value>(&line)
    {
        valid_path(&message)
    } else {
        None
    };
    cell.set(path);
    let mut input = limited.into_inner();
    let _ = std::io::copy(&mut input, &mut std::io::sink());
    let _ = parent_end.send(());
}

fn valid_path(message: &Value) -> Option<String> {
    let path = message.get("path").and_then(Value::as_str)?;
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.chars().any(|part| matches!(part, '\0' | '\n' | '\r'))
    {
        return None;
    }
    Some(path.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn read(bytes: &[u8]) -> (LoginPath, oneshot::Receiver<()>) {
        let cell = LoginPath::new();
        cell.expect(WAIT);
        let (parent_end, ended) = oneshot::channel();
        read_into_cell(&cell, Cursor::new(bytes.to_vec()), parent_end);
        (cell, ended)
    }

    #[test]
    fn a_valid_path_message_is_set_and_waited_for_no_more() {
        let (cell, mut ended) = read(b"{\"path\":\"/opt/homebrew/bin:/usr/bin\"}\nignored\n");
        assert_eq!(cell.wait().as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
        assert!(ended.try_recv().is_ok());
    }

    #[test]
    fn a_closed_pipe_or_a_bad_message_keeps_the_inherited_path_at_once() {
        let large = format!("{{\"path\":\"{}\"}}\n", "x".repeat(MAX_MESSAGE_BYTES));
        let long = format!("{{\"path\":\"{}\"}}\n", "x".repeat(MAX_PATH_BYTES + 1));
        for bytes in [
            Vec::new(),
            b"not json\n".to_vec(),
            b"{\"path\":\"/bin\"}".to_vec(),
            b"{\"path\":\"/bin\\n/evil\"}\n".to_vec(),
            b"{\"path\":\"\"}\n".to_vec(),
            large.into_bytes(),
            long.into_bytes(),
        ] {
            let (cell, mut ended) = read(&bytes);
            assert_eq!(cell.wait(), None);
            assert!(ended.try_recv().is_ok());
        }
    }

    #[test]
    fn the_shell_path_is_bin_dir_then_the_login_path() {
        let separator = if cfg!(windows) { ';' } else { ':' };
        assert_eq!(
            path_with_bin_dir(Some("/app/bin"), "/opt/bin"),
            format!("/app/bin{separator}/opt/bin")
        );
        assert_eq!(path_with_bin_dir(None, "/opt/bin"), "/opt/bin");
        // Not pending in tests: the env stays as it is, without a wait.
        let mut env: Env = [("PATH".to_string(), "/usr/bin".to_string())].into();
        use_login_path(&mut env, Some("/app/bin"));
        assert_eq!(env["PATH"], "/usr/bin");
    }

    #[test]
    fn expect_removes_the_variable_and_only_a_one_means_pending() {
        let mut plain = Env::new();
        assert!(!expect_login_path(&mut plain));
        let mut off: Env = [(PENDING.to_string(), "0".to_string())].into();
        assert!(!expect_login_path(&mut off));
        assert!(!off.contains_key(PENDING));
    }
}
