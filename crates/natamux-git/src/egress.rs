//! Remote calls under egress `allowlist`: the URL git would really use is resolved first (with
//! `insteadOf` rules applied), checked against `network.git`, and then git runs with that URL, so
//! a config change after the check cannot move the call. With egress `open` the old call runs.

use std::path::Path;
use std::time::Duration;

use natamux_core::egress::{Target, enforce};
use natamux_core::js::js_trim;
use natamux_core::security::{Egress, network_for};

use crate::exec::{GITHUB_CREDENTIALS, GitError, git, run_checked, with_git_config};
use crate::process::text_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteOp {
    Fetch,
    Push,
}

fn get_url(cwd: &Path, op: RemoteOp, remote: &str) -> Result<Vec<String>, GitError> {
    let mut args = vec!["remote", "get-url", "--all"];
    if op == RemoteOp::Push {
        args.push("--push");
    }
    args.push(remote);
    let out = git(cwd, &args, None, &[])?;
    Ok(js_trim(&out).lines().map(str::to_string).collect())
}

/// `url` after the `insteadOf` rules. `--get-url` only prints it: no network, so it passes the
/// backstop.
fn rewrite(cwd: &Path, url: &str) -> Result<Vec<String>, GitError> {
    let out = run_checked(cwd, &["ls-remote", "--get-url", url], None, &[], true).map(text_of)?;
    Ok(js_trim(&out).lines().map(str::to_string).collect())
}

/// A `pushInsteadOf` rule that would change `url` when git pushes to it.
fn push_rewrites(cwd: &Path, url: &str) -> bool {
    let rules = ["config", "--get-regexp", r"^url\..*\.pushinsteadof$"];
    // git exits 1 when no rule exists.
    let Ok(out) = git(cwd, &rules, None, &[]) else {
        return false;
    };
    out.lines()
        .filter_map(|line| line.split_once(' '))
        .any(|(_, prefix)| url.starts_with(prefix))
}

/// The one URL a call to `remote` (a name or a URL) reaches. More than one push URL is refused:
/// git would push to each.
pub fn resolve_remote(cwd: &Path, op: RemoteOp, remote: &str) -> Result<String, GitError> {
    let urls = match get_url(cwd, op, remote) {
        Ok(urls) => urls,
        Err(_) => rewrite(cwd, remote)?,
    };
    let [url] = urls.as_slice() else {
        return Err(GitError(format!(
            "egress policy: the remote has {} URLs; Natamux sends only to one",
            urls.len()
        )));
    };
    // Git applies the rules again to the URL it is given: it must stay the same.
    let again = rewrite(cwd, url)?;
    let pushed_again = op == RemoteOp::Push && push_rewrites(cwd, url);
    if again.as_slice() != std::slice::from_ref(url) || pushed_again {
        return Err(GitError(
            "egress policy: a URL rule rewrites the remote URL twice".to_string(),
        ));
    }
    Ok(url.clone())
}

// Global options that take their value as the next argument (`offline.rs` has the same).
const OPTIONS_WITH_VALUE: [&str; 6] = [
    "-c",
    "-C",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
];

/// The `network.git` action of a git call: `fetch`, `push`, `push-force` or `delete`. None for
/// any other subcommand.
pub fn classify_op(args: &[&str]) -> Option<&'static str> {
    let mut rest = args.iter().copied();
    let sub = loop {
        let arg = rest.next()?;
        if OPTIONS_WITH_VALUE.contains(&arg) {
            rest.next();
        } else if !arg.starts_with('-') {
            break arg;
        }
    };
    match sub {
        "fetch" | "ls-remote" | "pull" => Some("fetch"),
        "push" => {
            let rest: Vec<&str> = rest.collect();
            let any = |test: &dyn Fn(&str) -> bool| rest.iter().any(|arg| test(arg));
            if any(&|arg| arg == "--delete" || arg == "-d" || arg.starts_with(':')) {
                Some("delete")
            } else if any(&|arg| {
                matches!(arg, "--force" | "-f" | "--mirror")
                    || arg.starts_with("--force-with-lease")
                    || arg.starts_with('+')
            }) {
                Some("push-force")
            } else {
                Some("push")
            }
        }
        _ => None,
    }
}

/// Runs a call to the resolved `url`: refused unless `args` name exactly that URL and the action
/// is allowed. Git runs no hook, takes only https, ssh and file, and asks no credential helper
/// but the GitHub CLI.
pub fn run_pinned(
    cwd: &Path,
    url: &str,
    args: &[&str],
    timeout: Option<Duration>,
) -> Result<String, GitError> {
    let Some(action) = classify_op(args) else {
        return Err(GitError("egress policy: not a remote call".into()));
    };
    if !args.contains(&url) {
        return Err(GitError(format!(
            "egress policy: git {action} without its checked URL"
        )));
    }
    enforce(&network_for(cwd), &Target::Git { url, action }).map_err(GitError)?;
    let hooks = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let hooks = format!("core.hooksPath={hooks}");
    let mut all = vec!["-c", hooks.as_str()];
    all.extend(args);
    let mut entries = vec![("credential.helper", "")];
    entries.extend(GITHUB_CREDENTIALS);
    let mut env = with_git_config(std::env::var("GIT_CONFIG_COUNT").ok().as_deref(), &entries);
    env.push(("GIT_ALLOW_PROTOCOL".into(), "https:ssh:file".into()));
    run_checked(cwd, &all, timeout, &env, true).map(text_of)
}

/// A remote call. `args` gets the remote (or its checked URL) and whether it is pinned, so a
/// caller can name the remote-tracking ref a URL call does not update by itself. With egress
/// `open` (and offline, which the backstop refuses) the old call runs with `env`.
pub fn git_remote(
    cwd: &Path,
    op: RemoteOp,
    remote: &str,
    args: &dyn Fn(&str, bool) -> Vec<String>,
    timeout: Option<Duration>,
    env: &[(String, String)],
) -> Result<String, GitError> {
    if network_for(cwd).egress != Egress::Allowlist {
        let args = args(remote, false);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        return git(cwd, &args, timeout, env);
    }
    let url = resolve_remote(cwd, op, remote)?;
    let args = args(&url, true);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_pinned(cwd, &url, &args, timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_call_gets_its_action() {
        assert_eq!(
            classify_op(&["fetch", "--quiet", "u", "main"]),
            Some("fetch")
        );
        assert_eq!(classify_op(&["ls-remote", "--heads", "u"]), Some("fetch"));
        assert_eq!(
            classify_op(&["push", "--set-upstream", "u", "b"]),
            Some("push")
        );
        assert_eq!(
            classify_op(&["push", "--force", "u", "a:b"]),
            Some("push-force")
        );
        assert_eq!(classify_op(&["push", "u", "+a:b"]), Some("push-force"));
        assert_eq!(
            classify_op(&["push", "--force-with-lease=b", "u"]),
            Some("push-force")
        );
        assert_eq!(classify_op(&["push", "u", "--delete", "b"]), Some("delete"));
        assert_eq!(classify_op(&["push", "u", ":b"]), Some("delete"));
        assert_eq!(classify_op(&["-c", "a=b", "push", "u", "b"]), Some("push"));
        assert_eq!(classify_op(&["status"]), None);
        assert_eq!(classify_op(&["remote", "update"]), None);
    }
}
