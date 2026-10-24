//! Ports `packages/daemon/src/git/branches.ts`: the branches of a repository, and the deletes
//! of a branch that is done with.

use std::path::Path;

use natamux_core::js::js_trim;
use natamux_proto::projects::repo::BranchList;
use serde_json::{Value, json};

use crate::changes::locale_compare;
use crate::exec::{GitError, current_branch, git};

/// `GET /git/branches` as Node sends it: `branches`, `current`, `defaultBranch`, in this order.
pub fn branch_list_json(list: &BranchList) -> Value {
    json!({
        "branches": list.branches,
        "current": list.current,
        "defaultBranch": list.default_branch,
    })
}

/// The local branches of a repository, sorted, the one its checkout is on, and its default one.
pub fn list_branches(repo: &Path) -> Result<BranchList, GitError> {
    let output = git(
        repo,
        &["for-each-ref", "--format=%(refname:short)", "refs/heads"],
        None,
        &[],
    )?;
    let mut branches: Vec<String> = output
        .split('\n')
        .map(js_trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    branches.sort_by(|a, b| locale_compare(a, b));
    let current = current_branch(repo);
    let default_branch = default_branch(repo, &branches, current.as_deref());
    Ok(BranchList {
        branches,
        current,
        default_branch,
    })
}

// The default branch of origin, then main, then master, then the checkout's branch. Reads
// origin/HEAD as the clone left it: no network.
fn default_branch(repo: &Path, branches: &[String], current: Option<&str>) -> Option<String> {
    let args = [
        "symbolic-ref",
        "--quiet",
        "--short",
        "refs/remotes/origin/HEAD",
    ];
    let from_origin = git(repo, &args, None, &[]).ok().map(|name| {
        let name = js_trim(&name);
        name.strip_prefix("origin/").unwrap_or(name).to_string()
    });
    [from_origin.as_deref(), Some("main"), Some("master")]
        .into_iter()
        .flatten()
        .find(|name| branches.iter().any(|branch| branch == name))
        .or(current)
        .map(str::to_string)
}

/// Deletes the branch only when it is fully merged. Returns false when git keeps it.
pub fn delete_merged_branch(repo: &Path, branch: &str) -> bool {
    git(repo, &["branch", "-d", branch], None, &[]).is_ok()
}

/// Deletes a branch when origin has every commit of it, as last pushed or fetched, merged or not.
/// For a branch that GitHub merged with a squash or a rebase, which `git branch -d` keeps.
pub fn delete_pushed_branch(repo: &Path, branch: &str) -> bool {
    let range = format!("refs/remotes/origin/{branch}..refs/heads/{branch}");
    let Ok(ahead) = git(repo, &["rev-list", "--count", &range], None, &[]) else {
        return false;
    };
    if js_trim(&ahead) != "0" {
        return false;
    }
    git(repo, &["branch", "-D", branch], None, &[]).is_ok()
}

#[cfg(test)]
#[path = "branches_tests.rs"]
mod tests;
