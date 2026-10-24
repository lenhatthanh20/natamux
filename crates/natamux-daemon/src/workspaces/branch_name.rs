//! `branch-name.ts`: the branch of a task, and the delete of a branch that is done with.

use std::path::Path;

use natamux_git::branches::{delete_merged_branch, delete_pushed_branch};
use natamux_proto::tasks::task::PullRequestStatus;

use crate::shared::slug::slugify;

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

pub fn branch_name_for(id: &str, title: &str) -> String {
    format!("natamux/{}-{}", id.to_lowercase(), slugify(title))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_branch_name_from_the_task_id_and_title() {
        assert_eq!(
            branch_name_for("T-0042", "Fix the bug"),
            "natamux/t-0042-fix-the-bug"
        );
    }
}
