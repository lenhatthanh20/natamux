//! The offline switch for git: a call that talks to a remote is refused before git starts. Under
//! egress `loopback` the same; under `allowlist` only a call through `crate::egress` (its URL
//! resolved and checked) may run.

use std::path::Path;

use natamux_core::security::{Egress, network_for};

use crate::exec::GitError;

// Global options that take their value as the next argument.
const OPTIONS_WITH_VALUE: [&str; 6] = [
    "-c",
    "-C",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
];

/// The subcommand of a git call that connects to a remote, or None. The subcommand is the first
/// argument that is not a global option (`-c key=value`, `-C dir`, `--no-pager` ...).
fn network_command<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut rest = args.iter().copied();
    while let Some(arg) = rest.next() {
        if OPTIONS_WITH_VALUE.contains(&arg) {
            rest.next();
        } else if !arg.starts_with('-') {
            let network = match arg {
                "fetch" | "pull" | "push" | "ls-remote" | "clone" => true,
                "remote" | "submodule" => rest.next() == Some("update"),
                _ => false,
            };
            return network.then_some(arg);
        }
    }
    None
}

/// True when git, run with `args`, would connect to a remote.
pub fn uses_network(args: &[&str]) -> bool {
    network_command(args).is_some()
}

/// The error of a network call while offline, or None when git may run.
pub fn refusal(args: &[&str]) -> Option<GitError> {
    if !natamux_core::offline::offline() {
        return None;
    }
    network_command(args).map(|name| {
        GitError(format!(
            "git {name} failed: offline mode: the network is not reached"
        ))
    })
}

/// The refusal of a network call by the offline switch or the egress setting, or None.
pub fn backstop(cwd: &Path, args: &[&str], pinned: bool) -> Option<GitError> {
    let name = network_command(args)?;
    if let Some(refused) = refusal(args) {
        return Some(refused);
    }
    let reason = match network_for(cwd).egress {
        Egress::Open => return None,
        Egress::Allowlist if pinned => return None,
        Egress::Allowlist => "egress policy: a remote call goes only through a checked remote",
        Egress::Loopback => "egress policy: loopback only, so the network is not reached",
    };
    eprintln!("{reason} (git {name})");
    Some(GitError(format!("git {name} failed: {reason}")))
}

#[cfg(test)]
#[path = "offline_tests.rs"]
mod tests;
