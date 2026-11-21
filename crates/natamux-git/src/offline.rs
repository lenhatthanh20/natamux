//! The offline switch for git: a call that talks to a remote is refused before git starts.

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

#[cfg(test)]
#[path = "offline_tests.rs"]
mod tests;
