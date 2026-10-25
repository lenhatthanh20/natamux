//! Ports `packages/daemon/src/git/graph.ts`: the start of a branch, the history of a task branch,
//! and where it stands against `origin`.

use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use natamux_core::js::js_trim;
use natamux_proto::workspaces::changes::MAX_GRAPH_COMMITS;
use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit, PushStatus};

use crate::exec::{GitError, current_branch, git};

/// `FIELD`: the separator of the fields of one log record.
pub const FIELD: &str = "\u{1f}";
const LOG_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%aI%x1f%D%x1f%s";

/// A fetch that hangs (no network) must not hold up a new workspace for long.
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

fn run(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    git(cwd, args, None, &[])
}

/// The remote branch that `branch` follows, such as "origin/main". None without one.
fn upstream_of(repo_dir: &Path, branch: &str) -> Option<String> {
    let name = format!("{branch}@{{upstream}}");
    let out = run(
        repo_dir,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", &name],
    )
    .ok()?;
    let name = js_trim(&out);
    (!name.is_empty()).then(|| name.to_string())
}

/// Where new work from `branch` starts: its remote branch when the local branch is behind it, so
/// a branch that was not pulled does not start tasks from old code. Local commits that are not
/// pushed win. None: the local branch.
pub fn start_ref(repo_dir: &Path, branch: &str) -> Option<String> {
    let upstream = upstream_of(repo_dir, branch)?;
    run(
        repo_dir,
        &["merge-base", "--is-ancestor", branch, &upstream],
    )
    .ok()?;
    Some(upstream)
}

/// Brings the remote branch of `branch` up to date. The local branch does not move. Without a
/// network, or a remote, the work starts from what is known, so a failure is not an error.
pub fn fetch_upstream(repo_dir: &Path, branch: &str) {
    let fetch = || -> Result<(), GitError> {
        let remote = run(
            repo_dir,
            &["config", "--get", &format!("branch.{branch}.remote")],
        )?;
        let merge = run(
            repo_dir,
            &["config", "--get", &format!("branch.{branch}.merge")],
        )?;
        git(
            repo_dir,
            &["fetch", "--quiet", js_trim(&remote), js_trim(&merge)],
            Some(FETCH_TIMEOUT),
            &[],
        )?;
        Ok(())
    };
    let _ = fetch();
}

/// "HEAD -> natamux/t-0001, tag: v1, main" becomes ["natamux/t-0001", "tag: v1", "main"].
/// "origin/HEAD" is left out: it only repeats the default branch of the remote.
pub fn parse_refs(decoration: &str) -> Vec<String> {
    decoration
        .split(", ")
        .map(js_trim)
        .filter(|name| !name.is_empty() && *name != "HEAD" && !name.ends_with("/HEAD"))
        .map(|name| name.strip_prefix("HEAD -> ").unwrap_or(name).to_string())
        .collect()
}

/// Reads `git log -z` in `LOG_FORMAT`: one NUL-ended record per commit, fields split by `FIELD`.
pub fn parse_log(output: &str, is_on_branch: &dyn Fn(&str) -> bool) -> Vec<GraphCommit> {
    let mut commits = Vec::new();
    for record in output.split('\0') {
        let record = record.strip_prefix('\n').unwrap_or(record);
        let fields: Vec<&str> = record.split(FIELD).collect();
        if fields.len() < 6 || fields[0].is_empty() {
            continue;
        }
        commits.push(GraphCommit {
            hash: fields[0].to_string(),
            parents: if fields[1].is_empty() {
                Vec::new()
            } else {
                fields[1].split(' ').map(str::to_string).collect()
            },
            author: fields[2].to_string(),
            date: fields[3].to_string(),
            subject: fields[5..].join(FIELD),
            refs: parse_refs(fields[4]),
            on_branch: is_on_branch(fields[0]),
        });
    }
    commits
}

pub fn lines(output: &str) -> Vec<&str> {
    output.split('\n').filter(|line| !line.is_empty()).collect()
}

/// Whether origin has `branch`, by the remote-tracking ref that a push or a fetch left. No network.
pub fn has_remote_branch(worktree: &Path, branch: &str) -> Result<bool, GitError> {
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let out = run(
        worktree,
        &["for-each-ref", "--format=%(refname)", &remote_ref],
    )?;
    Ok(lines(&out).contains(&remote_ref.as_str()))
}

/// `Number(x)` of a count that git printed: 0 for what is not a number.
fn count_of(text: &str) -> u64 {
    js_trim(text).parse().unwrap_or(0)
}

/// The two numbers of `rev-list --left-right --count`.
fn count_pair(output: &str) -> (u64, u64) {
    let mut numbers = output.split_whitespace();
    (
        numbers.next().map_or(0, count_of),
        numbers.next().map_or(0, count_of),
    )
}

/// Compares `branch` with its copy on origin. Uses the remote-tracking refs as they are: no fetch.
/// Not `@{upstream}`, because a branch made from origin/main tracks origin/main.
pub fn read_push_status(worktree: &Path, branch: &str) -> Result<PushStatus, GitError> {
    if !lines(&run(worktree, &["remote"])?).contains(&"origin") {
        return Ok(PushStatus::None);
    }
    if !has_remote_branch(worktree, branch)? {
        let count = run(
            worktree,
            &[
                "rev-list",
                "--count",
                "HEAD",
                "--not",
                "--remotes=origin",
                "--",
            ],
        )?;
        return Ok(PushStatus::New {
            unpushed: count_of(&count),
        });
    }
    let range = format!("HEAD...refs/remotes/origin/{branch}");
    let counts = run(
        worktree,
        &["rev-list", "--left-right", "--count", &range, "--"],
    )?;
    let (unpushed, unpulled) = count_pair(&counts);
    Ok(PushStatus::Tracked { unpushed, unpulled })
}

/// The history of the worktree's branch and of `base`, newest first, in the order git draws a
/// graph.
pub fn read_git_graph(
    worktree: &Path,
    base: Option<&str>,
    limit: usize,
) -> Result<GitGraph, GitError> {
    let head = js_trim(&run(worktree, &["rev-parse", "HEAD"])?).to_string();
    let branch = current_branch(worktree);
    let compare_to = base.filter(|base| Some(*base) != branch.as_deref());

    let max_count = format!("--max-count={}", limit + 1);
    let format = format!("--format={LOG_FORMAT}");
    let mut args = vec!["log", "-z", "--topo-order", &max_count, &format, "HEAD"];
    args.extend(compare_to);
    args.push("--");
    let log = run(worktree, &args)?;

    let mut ahead = None;
    let mut behind = None;
    let mut own: Option<HashSet<String>> = None;
    if let Some(compare_to) = compare_to {
        let range = format!("HEAD...{compare_to}");
        let counts = run(
            worktree,
            &["rev-list", "--left-right", "--count", &range, "--"],
        )?;
        let (left, right) = count_pair(&counts);
        ahead = Some(left);
        behind = Some(right);
        let since = format!("{compare_to}..HEAD");
        let listed = run(worktree, &["rev-list", &max_count, &since, "--"])?;
        own = Some(lines(&listed).into_iter().map(str::to_string).collect());
    }

    let is_on_branch = |hash: &str| own.as_ref().is_none_or(|set| set.contains(hash));
    let mut commits = parse_log(&log, &is_on_branch);
    let truncated = commits.len() > limit;
    commits.truncate(limit);
    let push = match &branch {
        Some(name) => Some(read_push_status(worktree, name)?),
        None => None,
    };
    Ok(GitGraph {
        branch,
        head,
        base: compare_to.map(str::to_string),
        ahead,
        behind,
        push,
        commits,
        truncated,
    })
}

/// `MAX_GRAPH_COMMITS`, for callers that take the default limit.
pub const DEFAULT_LIMIT: usize = MAX_GRAPH_COMMITS;

#[cfg(test)]
#[path = "graph_push_tests.rs"]
mod push_tests;
#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
