//! The environment of the programs Natamux starts for a task (terminals, stage checks, the
//! commit-draft agent). With `credentials.agentEnv: strip` (the Strict default) they lose the
//! user's git and GitHub credentials; the model keys stay, so the agent still works. With a proxy
//! set, they get it and `NO_PROXY` becomes loopback plus `noProxy`. In Strict mode they also get
//! the agents' opt-out variables (`STRICT_OPT_OUTS`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::security::{NetworkSettings, installed, mode, network};

pub type Env = BTreeMap<String, String>;

pub const STRIPPED: [&str; 10] = [
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "GH_ENTERPRISE_TOKEN",
    "GITHUB_ENTERPRISE_TOKEN",
    "GITLAB_TOKEN",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "SSH_AUTH_SOCK",
    "GIT_SSH",
    "GIT_SSH_COMMAND",
];
const STRIPPED_PREFIX: &str = "GIT_CREDENTIAL_";
const PROXY_NAMES: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
const LOOPBACK_HOSTS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];
/// The opt-outs set in Strict mode: the common one and Claude Code's; no guessed names.
pub const STRICT_OPT_OUTS: [(&str, &str); 2] = [
    ("DO_NOT_TRACK", "1"),
    ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
];

/// `withGitConfig`: the `GIT_CONFIG_*` variables that add `entries` after the ones the
/// environment has (`current_count` is its `GIT_CONFIG_COUNT`).
pub fn with_git_config(
    current_count: Option<&str>,
    entries: &[(&str, &str)],
) -> Vec<(String, String)> {
    let start: usize = current_count
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(0);
    let mut env = vec![(
        "GIT_CONFIG_COUNT".to_string(),
        (start + entries.len()).to_string(),
    )];
    for (offset, (key, value)) in entries.iter().enumerate() {
        env.push((
            format!("GIT_CONFIG_KEY_{}", start + offset),
            key.to_string(),
        ));
        env.push((
            format!("GIT_CONFIG_VALUE_{}", start + offset),
            value.to_string(),
        ));
    }
    env
}

/// What to change in an environment: names to drop, then pairs to set.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub remove: Vec<String>,
    pub set: Vec<(String, String)>,
}

/// The folder `GH_CONFIG_DIR` points to while stripped: never written by Natamux, so gh finds
/// no login there.
pub fn empty_gh_dir() -> PathBuf {
    installed()
        .and_then(|source| source.path().parent().map(Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir)
        .join("gh-none")
}

/// The changes `settings` make to `env`. Empty for `inherit`, no proxy and Personal mode:
/// today's environment.
pub fn changes(settings: &NetworkSettings, strict: bool, env: &Env, gh_dir: &Path) -> Changes {
    let mut out = Changes::default();
    if strict {
        for (name, value) in STRICT_OPT_OUTS {
            // A value the user set wins, so a user can still choose another one.
            if !env.contains_key(name) {
                out.set.push((name.into(), value.into()));
            }
        }
    }
    if settings.strip {
        let named = |name: &str| {
            STRIPPED.contains(&name)
                || name.starts_with(STRIPPED_PREFIX)
                || settings.strip_extra.iter().any(|extra| extra == name)
        };
        out.remove = env.keys().filter(|name| named(name)).cloned().collect();
        for name in STRIPPED.iter().map(|name| name.to_string()) {
            if !out.remove.contains(&name) {
                out.remove.push(name);
            }
        }
        for name in &settings.strip_extra {
            if !out.remove.contains(name) {
                out.remove.push(name.clone());
            }
        }
        let gh_dir = gh_dir.to_string_lossy().into_owned();
        out.set.push(("GH_CONFIG_DIR".into(), gh_dir));
        out.set.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
        out.set.push(("GCM_INTERACTIVE".into(), "never".into()));
        let count = env.get("GIT_CONFIG_COUNT").map(String::as_str);
        out.set
            .extend(with_git_config(count, &[("credential.helper", "")]));
    }
    if let Some(proxy) = &settings.proxy {
        for name in PROXY_NAMES {
            out.set.push((name.into(), proxy.clone()));
        }
        let mut hosts: Vec<String> = LOOPBACK_HOSTS.map(str::to_string).to_vec();
        hosts.extend(settings.no_proxy.iter().cloned());
        let list = hosts.join(",");
        out.set.push(("NO_PROXY".into(), list.clone()));
        out.set.push(("no_proxy".into(), list));
    }
    out
}

/// `env` changed by `settings` and the security mode.
pub fn scrub_with(settings: &NetworkSettings, strict: bool, env: &Env, gh_dir: &Path) -> Env {
    let changes = changes(settings, strict, env, gh_dir);
    let mut out = env.clone();
    for name in &changes.remove {
        out.remove(name);
    }
    out.extend(changes.set);
    out
}

/// `env` changed by the network settings now.
pub fn scrub(env: &Env) -> Env {
    scrub_with(&network(), mode().is_strict(), env, &empty_gh_dir())
}

/// The environment `command` starts with: the daemon's, changed by what was set on it.
fn env_of(command: &Command) -> Env {
    let mut env: Env = std::env::vars().collect();
    for (name, value) in command.get_envs() {
        let name = name.to_string_lossy().into_owned();
        match value {
            Some(value) => env.insert(name, value.to_string_lossy().into_owned()),
            None => env.remove(&name),
        };
    }
    env
}

/// `scrub` for a command that inherits the daemon's environment.
pub fn scrub_command(command: &mut Command) {
    let strict = mode().is_strict();
    let changes = changes(&network(), strict, &env_of(command), &empty_gh_dir());
    for name in &changes.remove {
        command.env_remove(name);
    }
    command.envs(changes.set);
}

#[cfg(test)]
#[path = "child_env_tests.rs"]
mod tests;
