//! `branch-name.ts`: the branch of a task, and the delete of a branch that is done with.

use std::path::Path;

use natamux_git::branches::{delete_merged_branch, delete_pushed_branch};
use natamux_proto::tasks::task::PullRequestStatus;

pub use natamux_core::workspaces::branch_name::{
    branch_name_for, branch_name_from, is_branch_name,
};

/// `git branch -d` keeps a branch that GitHub merged with a squash or a rebase. A pull request that
/// says it is merged lets it go too, once origin has all of its commits, so no work is lost.
pub fn delete_finished_branch(
    repo: &Path,
    branch: &str,
    pull_request: Option<&PullRequestStatus>,
) -> bool {
    if delete_merged_branch(repo, branch) {
        return true;
    }
    pull_request.is_some_and(|status| status.state == "merged")
        && delete_pushed_branch(repo, branch)
}
