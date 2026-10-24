//! The daemon's settings from its environment (`config.ts`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use natamux_pty::Env;

use crate::terminals::session_lifecycle::random_id;

pub const DEFAULT_PORT: u16 = 4317;
pub const LOOPBACK_HOST: &str = "127.0.0.1";

pub struct Config {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub worktrees_dir: PathBuf,
    pub token: String,
    pub token_was_generated: bool,
    /// The token of the task shells. Only made here, so an agent cannot set it.
    pub agent_token: String,
    pub silent_close: std::time::Duration,
    pub agents: AgentConfig,
}

/// How agents start: `NATAMUX_<AGENT>_COMMAND` and `_ARGS` (the agent id in capitals, `CLAUDE`
/// for Claude Code), and `NATAMUX_BIN_DIR`, the folder of the app's `natamux` command.
#[derive(Clone, Debug, Default)]
pub struct AgentConfig {
    pub programs: HashMap<String, String>,
    pub extra_args: HashMap<String, String>,
    pub bin_dir: Option<String>,
}

fn agent_config(env: &Env) -> AgentConfig {
    let mut config = AgentConfig {
        bin_dir: value(env, "NATAMUX_BIN_DIR").map(str::to_string),
        ..AgentConfig::default()
    };
    for agent in natamux_proto::agents::start::LAUNCHABLE_AGENTS {
        let name = if agent == "claude-code" {
            "CLAUDE".to_string()
        } else {
            agent.to_uppercase()
        };
        if let Some(program) = env.get(&format!("NATAMUX_{name}_COMMAND")) {
            config.programs.insert(agent.into(), program.clone());
        }
        if let Some(words) = env.get(&format!("NATAMUX_{name}_ARGS")) {
            config.extra_args.insert(agent.into(), words.clone());
        }
    }
    config
}

fn value<'a>(env: &'a Env, name: &str) -> Option<&'a str> {
    env.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

pub fn load_config(env: &Env, cwd: &Path) -> Result<Config, String> {
    let port = match value(env, "NATAMUX_PORT") {
        None => DEFAULT_PORT,
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|port| port.fract() == 0.0 && (0.0..=65535.0).contains(port))
            .map(|port| port as u16)
            .ok_or_else(|| format!("NATAMUX_PORT must be a port number, got \"{raw}\""))?,
    };
    let data_dir =
        value(env, "NATAMUX_DATA_DIR").map_or_else(|| cwd.join(".natamux"), PathBuf::from);
    let worktrees_dir = value(env, "NATAMUX_WORKTREES_DIR")
        .map_or_else(|| data_dir.join("worktrees"), PathBuf::from);
    let configured = value(env, "NATAMUX_TOKEN");
    Ok(Config {
        // Only a container sets NATAMUX_HOST, and only with its port published to loopback.
        host: value(env, "NATAMUX_HOST")
            .unwrap_or(LOOPBACK_HOST)
            .to_string(),
        port,
        data_dir,
        worktrees_dir,
        token: configured.map_or_else(|| random_id(24), String::from),
        token_was_generated: configured.is_none(),
        agent_token: random_id(24),
        silent_close: natamux_proto::terminals::messages::SILENT_CLOSE,
        agents: agent_config(env),
    })
}

/// The environment of task shells: the agent token, never the daemon's own, so an agent cannot
/// pass a gate. With no locale (the Docker image, an app started from the Dock) a shell reads
/// non-ASCII input, such as Vietnamese, as Meta keys and garbles it.
pub fn shell_env(env: &Env, agent_token: &str, daemon_url: &str) -> Env {
    let mut out = env.clone();
    let no_locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .all(|name| env.get(*name).is_none_or(|value| value.is_empty()));
    if no_locale {
        out.insert("LANG".into(), "C.UTF-8".into());
    }
    out.insert("NATAMUX_TOKEN".into(), agent_token.into());
    out.insert("NATAMUX_URL".into(), daemon_url.into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Env {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn reads_the_settings_with_their_defaults() {
        let config = load_config(&env(&[]), Path::new("/work")).unwrap();
        assert_eq!((config.host.as_str(), config.port), ("127.0.0.1", 4317));
        assert_eq!(config.data_dir, Path::new("/work/.natamux"));
        assert_eq!(config.worktrees_dir, Path::new("/work/.natamux/worktrees"));
        assert!(config.token_was_generated && config.token.len() == 32);
        assert_ne!(config.token, config.agent_token);

        let set = env(&[
            ("NATAMUX_PORT", " 8080 "),
            ("NATAMUX_HOST", "0.0.0.0"),
            ("NATAMUX_DATA_DIR", "/data"),
            ("NATAMUX_TOKEN", "secret"),
        ]);
        let config = load_config(&set, Path::new("/work")).unwrap();
        assert_eq!((config.host.as_str(), config.port), ("0.0.0.0", 8080));
        assert_eq!(config.worktrees_dir, Path::new("/data/worktrees"));
        assert_eq!(
            (config.token.as_str(), config.token_was_generated),
            ("secret", false)
        );
    }

    #[test]
    fn reads_the_command_and_words_of_each_agent() {
        let set = env(&[
            ("NATAMUX_CLAUDE_COMMAND", "/opt/fake-agent.sh"),
            ("NATAMUX_CODEX_ARGS", "--model x"),
            ("NATAMUX_BIN_DIR", "/app/bin"),
        ]);
        let config = load_config(&set, Path::new("/")).unwrap().agents;
        assert_eq!(config.programs["claude-code"], "/opt/fake-agent.sh");
        assert_eq!(config.extra_args["codex"], "--model x");
        assert_eq!(config.programs.len() + config.extra_args.len(), 2);
        assert_eq!(config.bin_dir.as_deref(), Some("/app/bin"));
    }

    #[test]
    fn refuses_a_port_that_is_not_one() {
        for bad in ["http", "70000", "1.5", "-1"] {
            let error = load_config(&env(&[("NATAMUX_PORT", bad)]), Path::new("/")).err();
            assert_eq!(
                error,
                Some(format!("NATAMUX_PORT must be a port number, got \"{bad}\""))
            );
        }
    }

    #[test]
    fn shells_get_the_agent_token_the_address_and_a_locale() {
        let out = shell_env(
            &env(&[("NATAMUX_TOKEN", "daemon")]),
            "agent",
            "http://127.0.0.1:4317",
        );
        assert_eq!(out["NATAMUX_TOKEN"], "agent");
        assert_eq!(out["NATAMUX_URL"], "http://127.0.0.1:4317");
        assert_eq!(out["LANG"], "C.UTF-8");
        let set = shell_env(&env(&[("LC_ALL", "vi_VN.UTF-8")]), "agent", "u");
        assert!(!set.contains_key("LANG"));
    }
}
