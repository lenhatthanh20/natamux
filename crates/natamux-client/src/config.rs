//! Where the daemon is and how to sign in, as the `natamux` command finds it.

use std::path::{Path, PathBuf};

use crate::error::ClientError;

/// The daemon's default address (`DEFAULT_URL` in `packages/cli/src/default-url.ts`).
pub const DEFAULT_URL: &str = "http://127.0.0.1:4317";

/// The file where the Electron app keeps the daemon's token, inside its user data folder.
const ELECTRON_TOKEN_FILE: &str = "daemon-token";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// `http://host:port`, with any path prefix such as `/api`, and no `/` at the end.
    pub base_url: String,
    pub token: String,
}

impl Config {
    /// `NATAMUX_URL` and `NATAMUX_TOKEN`. Without them: the default address and the token of the
    /// Electron app on this machine, so the app finds the daemon that the Electron app started.
    pub fn from_env() -> Result<Config, ClientError> {
        resolve(
            |name| std::env::var(name).ok(),
            electron_token_file().as_deref(),
            |path| std::fs::read_to_string(path).ok(),
        )
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

fn resolve(
    env: impl Fn(&str) -> Option<String>,
    token_file: Option<&Path>,
    read: impl Fn(&Path) -> Option<String>,
) -> Result<Config, ClientError> {
    let set = |name: &str| {
        env(name)
            .map(|value| value.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let base_url = set("NATAMUX_URL")
        .unwrap_or_else(|| DEFAULT_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    if !base_url.starts_with("http://") {
        return Err(ClientError::BadUrl(base_url));
    }
    let token = set("NATAMUX_TOKEN")
        .or_else(|| {
            token_file
                .and_then(&read)
                .map(|text| text.trim().to_string())
        })
        .filter(|token| !token.is_empty())
        .ok_or(ClientError::NoToken)?;
    Ok(Config { base_url, token })
}

/// The Electron app's user data folder is the OS's app data folder plus "Natamux".
fn electron_token_file() -> Option<PathBuf> {
    let base = if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?
    };
    Some(base.join("Natamux").join(ELECTRON_TOKEN_FILE))
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
            Some(Path::new(FILE)),
            saved_token,
        )
        .unwrap();
        assert_eq!(config.base_url, "http://10.0.0.2:5000/api");
        assert_eq!(config.token, "abc");
    }

    #[test]
    fn falls_back_to_the_default_url_and_the_electron_token() {
        let config = resolve(env(&[]), Some(Path::new(FILE)), saved_token).unwrap();
        assert_eq!(config.base_url, DEFAULT_URL);
        assert_eq!(config.token, "from-the-file");
    }

    #[test]
    fn no_token_anywhere_is_an_error() {
        let none = |_: &Path| None;
        assert_eq!(
            resolve(env(&[("NATAMUX_TOKEN", "  ")]), None, none),
            Err(ClientError::NoToken)
        );
        let empty = |_: &Path| Some("\n".to_string());
        assert_eq!(
            resolve(env(&[]), Some(Path::new(FILE)), empty),
            Err(ClientError::NoToken)
        );
    }

    #[test]
    fn only_plain_http_is_taken() {
        let result = resolve(
            env(&[
                ("NATAMUX_URL", "https://example.com"),
                ("NATAMUX_TOKEN", "t"),
            ]),
            None,
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
