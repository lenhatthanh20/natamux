//! Where the daemon is and how to sign in, as the `natamux` command finds it.

use std::path::{Path, PathBuf};

use crate::error::ClientError;

/// The daemon's default address (`DEFAULT_URL` in `packages/cli/src/default-url.ts`).
pub const DEFAULT_URL: &str = "http://127.0.0.1:4317";

/// The app's daemon token, inside its user data folder.
const APP_TOKEN_FILE: &str = "daemon-token";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `http://host:port`, with any path prefix such as `/api`, and no `/` at the end.
    pub base_url: String,
    pub token: String,
}

impl Config {
    /// `NATAMUX_URL` and `NATAMUX_TOKEN`. Without them: the default address and the token of the
    /// Natamux app on this machine, so the CLI finds the daemon that the app started.
    /// In a task's terminal (`NATAMUX_TASK_ID` set), `NATAMUX_TOKEN` is that daemon run's agent
    /// token: it may not pass gates and is new after each daemon restart. There the saved token
    /// wins when there is one.
    pub fn from_env() -> Result<Config, ClientError> {
        Config::from_env_with(None)
    }

    /// `from_env` with the app's own saved token read first.
    pub fn from_env_with(app_file: Option<&Path>) -> Result<Config, ClientError> {
        let saved = app_token_file();
        let files: Vec<&Path> = app_file.into_iter().chain(saved.as_deref()).collect();
        let app_url = app_daemon_url();
        resolve(
            |name| {
                std::env::var(name)
                    .ok()
                    .or_else(|| (name == "NATAMUX_URL").then(|| app_url.clone()).flatten())
            },
            &files,
            |path| std::fs::read_to_string(path).ok(),
        )
    }

    /// The daemon's address with no token yet: Sign in starts from it when no token was found.
    pub fn without_token() -> Result<Config, ClientError> {
        let app_url = app_daemon_url();
        Ok(Config {
            base_url: base_url_of(|name| {
                std::env::var(name)
                    .ok()
                    .or_else(|| (name == "NATAMUX_URL").then(|| app_url.clone()).flatten())
            })?,
            token: String::new(),
        })
    }

    /// The same daemon with another token (the Sign in check).
    pub fn with_token(&self, token: &str) -> Config {
        Config {
            base_url: self.base_url.clone(),
            token: token.trim().to_string(),
        }
    }

    /// The `ws://` address of a daemon path, from `base_url`.
    pub fn ws_url(&self, path_and_query: &str) -> Result<String, ClientError> {
        let rest = self
            .base_url
            .strip_prefix("http://")
            .ok_or_else(|| ClientError::BadUrl(self.base_url.clone()))?;
        Ok(format!("ws://{rest}{path_and_query}"))
    }
}

/// Saves the token the user typed, so the next start finds it. The file is private to the user and
/// is replaced whole, never half written.
pub fn save_access(path: &Path, token: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    let partial = PathBuf::from(partial);
    let mut file = open_private(&partial)?;
    std::io::Write::write_all(&mut file, token.trim().as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&partial, path)
}

/// A new file only the user can read from the first byte on: a mode set after the write would
/// leave the token readable by others for a moment. A partial file left by a crash goes first,
/// as it may be wider.
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    forget_access(path)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// Removes the saved token. A file that is not there is fine.
pub fn forget_access(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn resolve(
    env: impl Fn(&str) -> Option<String>,
    token_files: &[&Path],
    read: impl Fn(&Path) -> Option<String>,
) -> Result<Config, ClientError> {
    let set = |name: &str| {
        env(name)
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let base_url = base_url_of(&env)?;
    let saved = || {
        token_files.iter().find_map(|path| {
            read(path)
                .map(|text| text.trim().to_string())
                .filter(|token| !token.is_empty())
        })
    };
    let token = if set("NATAMUX_TASK_ID").is_some() {
        saved().or_else(|| set("NATAMUX_TOKEN"))
    } else {
        set("NATAMUX_TOKEN").or_else(saved)
    }
    .ok_or(ClientError::NoToken)?;
    Ok(Config { base_url, token })
}

fn base_url_of(env: impl Fn(&str) -> Option<String>) -> Result<String, ClientError> {
    let base_url = env("NATAMUX_URL")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    if base_url.starts_with("http://") {
        Ok(base_url)
    } else {
        Err(ClientError::BadUrl(base_url))
    }
}

/// The app's user data folder is the OS's app data folder plus "Natamux".
fn app_token_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?
    };
    Some(base.join("Natamux").join(APP_TOKEN_FILE))
}

fn app_daemon_url() -> Option<String> {
    let file = app_token_file()?.with_file_name("daemon-port");
    url_from_port(&std::fs::read_to_string(file).ok()?)
}

fn url_from_port(text: &str) -> Option<String> {
    let port = text.trim().parse::<u16>().ok().filter(|port| *port > 0)?;
    Some(format!("http://127.0.0.1:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    }

    const FILE: &str = "/home/me/Natamux/daemon-token";

    fn saved_token(path: &Path) -> Option<String> {
        (path == Path::new(FILE)).then(|| "from-the-file\n".to_string())
    }

    #[test]
    fn the_env_wins() {
        let config = resolve(
            env(&[
                ("NATAMUX_URL", "http://10.0.0.2:5000/api/"),
                ("NATAMUX_TOKEN", " abc "),
            ]),
            &[Path::new(FILE)],
            saved_token,
        )
        .unwrap();
        assert_eq!(config.base_url, "http://10.0.0.2:5000/api");
        assert_eq!(config.token, "abc");
    }

    #[test]
    fn the_app_port_file_supplies_a_loopback_url_only_for_a_valid_port() {
        assert_eq!(
            url_from_port("21720\n"),
            Some("http://127.0.0.1:21720".into())
        );
        assert_eq!(url_from_port("0"), None);
        assert_eq!(url_from_port("65536"), None);
        assert_eq!(url_from_port("bad"), None);
    }

    #[test]
    fn falls_back_to_the_default_url_and_the_electron_token() {
        let config = resolve(env(&[]), &[Path::new(FILE)], saved_token).unwrap();
        assert_eq!(config.base_url, DEFAULT_URL);
        assert_eq!(config.token, "from-the-file");
    }

    // Gate 3 run of 2026-10-22: the app started from a task's terminal took the agent token, and
    // the daemon refused it after the Electron app restarted.
    #[test]
    fn in_a_task_terminal_the_saved_token_wins_over_the_agent_token() {
        let config = resolve(
            env(&[
                ("NATAMUX_TASK_ID", "T-0145"),
                ("NATAMUX_URL", "http://127.0.0.1:4317"),
                ("NATAMUX_TOKEN", "agent-token"),
            ]),
            &[Path::new(FILE)],
            saved_token,
        )
        .unwrap();
        assert_eq!(config.token, "from-the-file");
        assert_eq!(config.base_url, "http://127.0.0.1:4317");
        // No saved token, as against the Docker daemon: the env's token is all there is.
        let config = resolve(
            env(&[("NATAMUX_TASK_ID", "T-1"), ("NATAMUX_TOKEN", "agent-token")]),
            &[],
            |_| None,
        )
        .unwrap();
        assert_eq!(config.token, "agent-token");
    }

    #[test]
    fn no_token_anywhere_is_an_error() {
        let none = |_: &Path| None;
        assert_eq!(
            resolve(env(&[("NATAMUX_TOKEN", "  ")]), &[], none),
            Err(ClientError::NoToken)
        );
        let empty = |_: &Path| Some("\n".to_string());
        assert_eq!(
            resolve(env(&[]), &[Path::new(FILE)], empty),
            Err(ClientError::NoToken)
        );
    }

    const APP: &str = "/data/natamux/daemon-access";

    fn both(path: &Path) -> Option<String> {
        match path.to_str()? {
            APP => Some("from-the-app\n".into()),
            FILE => Some("from-the-file\n".into()),
            _ => None,
        }
    }

    fn files() -> [&'static Path; 2] {
        [Path::new(APP), Path::new(FILE)]
    }

    #[test]
    fn the_app_file_comes_before_the_electron_file() {
        let config = resolve(env(&[]), &files(), both).unwrap();
        assert_eq!(config.token, "from-the-app");
        // Outside a task terminal the env still wins.
        let config = resolve(env(&[("NATAMUX_TOKEN", "e")]), &files(), both).unwrap();
        assert_eq!(config.token, "e");
    }

    #[test]
    fn in_a_task_terminal_the_files_win_in_order_then_the_env() {
        let task = [("NATAMUX_TASK_ID", "T-1"), ("NATAMUX_TOKEN", "agent")];
        assert_eq!(
            resolve(env(&task), &files(), both).unwrap().token,
            "from-the-app"
        );
        let only_electron = |path: &Path| both(path).filter(|_| path == Path::new(FILE));
        assert_eq!(
            resolve(env(&task), &files(), only_electron).unwrap().token,
            "from-the-file"
        );
        assert_eq!(
            resolve(env(&task), &files(), |_| None).unwrap().token,
            "agent"
        );
    }

    #[test]
    fn a_blank_app_file_is_skipped() {
        let blank_app = |path: &Path| {
            if path == Path::new(APP) {
                Some(" \n".to_string())
            } else {
                both(path)
            }
        };
        let config = resolve(env(&[]), &files(), blank_app).unwrap();
        assert_eq!(config.token, "from-the-file");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("natamux-config-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_saved_token_reads_back_trimmed_and_forget_removes_it() {
        let dir = scratch("save");
        let path = dir.join("data").join("daemon-access");
        save_access(&path, "  abc123\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc123");
        assert!(!dir.join("data/daemon-access.partial").exists());
        save_access(&path, "next").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "next");
        forget_access(&path).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("mode");
        let path = dir.join("daemon-access");
        save_access(&path, "t").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Review 1: the token was written with the default mask and made private only after.
    #[cfg(unix)]
    #[test]
    fn the_partial_file_is_private_before_the_token_is_written() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("partial");
        std::fs::create_dir_all(&dir).unwrap();
        let partial = dir.join("daemon-access.partial");
        std::fs::write(&partial, "old").unwrap();
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o644)).unwrap();
        let file = open_private(&partial).unwrap();
        let mode = file.metadata().unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(file.metadata().unwrap().len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn forgetting_a_file_that_is_not_there_is_fine() {
        let dir = scratch("missing");
        assert!(forget_access(&dir.join("nothing")).is_ok());
    }

    #[test]
    fn only_plain_http_is_taken() {
        let result = resolve(
            env(&[
                ("NATAMUX_URL", "https://example.com"),
                ("NATAMUX_TOKEN", "t"),
            ]),
            &[],
            |_| None,
        );
        assert_eq!(
            result,
            Err(ClientError::BadUrl("https://example.com".into()))
        );
    }

    #[test]
    fn makes_websocket_urls() {
        let config = Config {
            base_url: "http://127.0.0.1:4317/api".into(),
            token: "t".into(),
        };
        assert_eq!(
            config.ws_url("/events?token=t").unwrap(),
            "ws://127.0.0.1:4317/api/events?token=t"
        );
    }
}
