//! The egress decision for Natamux's own network calls (git remotes, gh, the updater): pure, so
//! every caller and every test asks the same question. A refusal starts with `egress policy:`
//! and names the host and the action, never a URL that could hold a password.

use crate::security::{Egress, NetworkSettings, network};

pub const PREFIX: &str = "egress policy:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target<'a> {
    /// A git remote URL, as git would use it, and `fetch`, `push`, `push-force` or `delete`.
    Git {
        url: &'a str,
        action: &'a str,
    },
    /// A gh call to `host`: `read`, `pr-create` or `issue-create`.
    Gh {
        host: &'a str,
        action: &'a str,
    },
    Update {
        host: &'a str,
    },
}

/// Where a git remote URL goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Remote {
    /// A folder or `file://`: no network.
    Local,
    Network {
        host: String,
        owner: String,
        repo: String,
        password: bool,
    },
    /// A form Natamux does not let git use (`ext::`, an unknown scheme).
    Refused(String),
}

fn refused(reason: String) -> String {
    format!("{PREFIX} {reason}")
}

/// The owner and the repository of a remote path: the last part is the repository, the rest the
/// owner (`group/sub` on GitLab).
fn owner_repo(path: &str) -> (String, String) {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let Some((repo, owner)) = parts.split_last() else {
        return (String::new(), String::new());
    };
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    (owner.join("/"), repo.to_string())
}

fn host_only(authority: &str) -> String {
    let host = match authority.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    host.to_ascii_lowercase()
}

fn network_remote(authority: &str, path: &str) -> Remote {
    let (userinfo, host) = match authority.rsplit_once('@') {
        Some((userinfo, host)) => (Some(userinfo), host),
        None => (None, authority),
    };
    let host = host_only(host);
    if host.is_empty() {
        return Remote::Refused("a remote without a host is refused".into());
    }
    let (owner, repo) = owner_repo(path);
    Remote::Network {
        host,
        owner,
        repo,
        password: userinfo.is_some_and(|info| info.contains(':')),
    }
}

/// Reads a remote URL in the forms git takes: `https://`, `ssh://`, scp-like `user@host:path`,
/// `file://` and folder paths.
pub fn parse_remote(url: &str) -> Remote {
    let url = url.trim();
    if let Some((scheme, rest)) = url.split_once("://") {
        let scheme = scheme.to_ascii_lowercase();
        return match scheme.as_str() {
            "file" => Remote::Local,
            "https" | "ssh" | "git+ssh" | "ssh+git" => {
                let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
                network_remote(authority, path)
            }
            _ => Remote::Refused(format!("the remote scheme {scheme:?} is refused")),
        };
    }
    // `ext::cmd`, `fd::` and other `<transport>::<address>` forms run a helper.
    if url.contains("::") {
        return Remote::Refused("a remote helper (`<transport>::`) is refused".into());
    }
    let windows_drive = url.as_bytes().get(1) == Some(&b':')
        && url.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    if let Some((authority, path)) = url.split_once(':')
        && !authority.contains('/')
        && !windows_drive
        && !authority.is_empty()
    {
        return network_remote(authority, path);
    }
    Remote::Local
}

/// `rule` matches `host`: the same name, or `*.name` for any name below it.
pub fn host_matches(rule: &str, host: &str) -> bool {
    match rule.strip_prefix("*.") {
        Some(suffix) => host
            .to_ascii_lowercase()
            .strip_suffix(&suffix.to_ascii_lowercase())
            .is_some_and(|start| start.len() > 1 && start.ends_with('.')),
        None => rule.eq_ignore_ascii_case(host),
    }
}

fn part_matches(rule: &str, value: &str) -> bool {
    rule == "*" || rule.eq_ignore_ascii_case(value)
}

fn check_git(settings: &NetworkSettings, url: &str, action: &str) -> Result<(), String> {
    let (host, owner, repo) = match parse_remote(url) {
        Remote::Local => return Ok(()),
        Remote::Refused(reason) => return Err(refused(format!("git {action}: {reason}"))),
        Remote::Network { password: true, .. } => {
            return Err(refused(format!(
                "git {action}: a remote URL with a password is refused"
            )));
        }
        Remote::Network {
            host, owner, repo, ..
        } => (host, owner, repo),
    };
    let rules = settings
        .git
        .as_ref()
        .map_err(|bad| refused(format!("git {action} to {host}: {bad}")))?;
    let allowed = rules.iter().any(|rule| {
        host_matches(&rule.host, &host)
            && part_matches(&rule.owner, &owner)
            && part_matches(&rule.repo, &repo)
            && rule.actions.iter().any(|it| it == action)
    });
    if allowed {
        return Ok(());
    }
    Err(refused(format!(
        "git {action} to {host}/{owner}/{repo} is not in network.git"
    )))
}

fn check_gh(settings: &NetworkSettings, host: &str, action: &str) -> Result<(), String> {
    let rules = settings
        .gh
        .as_ref()
        .map_err(|bad| refused(format!("gh {action} on {host}: {bad}")))?;
    let allowed = rules
        .iter()
        .any(|rule| host_matches(&rule.host, host) && rule.actions.iter().any(|it| it == action));
    if allowed {
        return Ok(());
    }
    Err(refused(format!(
        "gh {action} on {host} is not in network.gh"
    )))
}

/// Whether Natamux may make the call `target` under `settings`.
pub fn check(settings: &NetworkSettings, target: &Target<'_>) -> Result<(), String> {
    if let Target::Update { .. } = target
        && !settings.updater
    {
        return Err(refused("updates are off (network.updater)".into()));
    }
    match settings.egress {
        Egress::Open => Ok(()),
        Egress::Loopback => Err(refused(format!(
            "loopback only, so {} is not reached",
            what(target)
        ))),
        Egress::Allowlist => match *target {
            Target::Git { url, action } => check_git(settings, url, action),
            Target::Gh { host, action } => check_gh(settings, host, action),
            Target::Update { host } => {
                if settings
                    .update_hosts
                    .iter()
                    .any(|rule| host_matches(rule, host))
                {
                    Ok(())
                } else {
                    Err(refused(format!("an update from {host} is not listed")))
                }
            }
        },
    }
}

fn what(target: &Target<'_>) -> String {
    match *target {
        Target::Git { url, action } => match parse_remote(url) {
            Remote::Network { host, .. } => format!("git {action} to {host}"),
            _ => format!("git {action}"),
        },
        Target::Gh { host, action } => format!("gh {action} on {host}"),
        Target::Update { host } => format!("an update from {host}"),
    }
}

/// `check` that logs a refusal as one line (host and action only).
pub fn enforce(settings: &NetworkSettings, target: &Target<'_>) -> Result<(), String> {
    check(settings, target).inspect_err(|reason| eprintln!("{reason}"))
}

/// Offline mode or egress `loopback`: Natamux reaches nothing beyond this machine.
pub fn loopback() -> bool {
    network().egress == Egress::Loopback
}

#[cfg(test)]
#[path = "egress_tests.rs"]
mod tests;
