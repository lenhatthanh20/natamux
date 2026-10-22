//! What a done or cancelled task still has to do with git, for the remove dialog. Port of
//! `branchChoice`, `busyShells`, `isAgentShell` and `removedText` in `packages/web/src/lib/finished.ts`.

use natamux_proto::tasks::task::{PullRequestStatus, Task};
use natamux_proto::terminals::list::{SHELL_PROGRAMS, TerminalList};
use natamux_proto::workspaces::workspace::{GitGraph, PushStatus};

/// The fields of a task these rules read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FinishedTask {
    pub pr: Option<String>,
    pub pr_status: Option<PullRequestStatus>,
    pub branch: Option<String>,
}

impl FinishedTask {
    pub fn from_task(task: &Task) -> Self {
        Self {
            pr: task.pr.clone(),
            pr_status: task.pull_request_status(),
            branch: task.branch.clone(),
        }
    }
}

/// The "Also delete the branch" box of the remove dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchChoice {
    pub can_delete: bool,
    pub note: String,
}

fn commits_are(count: u64) -> String {
    if count == 1 {
        "1 commit is".into()
    } else {
        format!("{count} commits are")
    }
}

// "Pull request #310", or "The pull request" for an address with no number.
fn pull_request_name(url: &str) -> String {
    let digits = url.split_once("/pull/").map(|(_, rest)| {
        rest.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
    });
    match digits {
        Some(number) if !number.is_empty() => format!("Pull request #{number}"),
        _ => "The pull request".into(),
    }
}

fn is_open_pull_request(task: &FinishedTask) -> bool {
    if task.pr.is_none() {
        return false;
    }
    // A pull request GitHub was not asked about yet counts as open.
    match &task.pr_status {
        None => true,
        Some(status) => status.state == "open" || status.state == "draft",
    }
}

fn unpushed(graph: &GitGraph) -> u64 {
    match graph.push {
        Some(PushStatus::New { unpushed } | PushStatus::Tracked { unpushed, .. }) => unpushed,
        _ => 0,
    }
}

// True when `git branch -d` takes the branch, or its merged pull request lets the daemon take it.
fn branch_merged(task: &FinishedTask, graph: &GitGraph) -> bool {
    if graph.ahead == Some(0) {
        return true;
    }
    let merged = task
        .pr_status
        .as_ref()
        .is_some_and(|status| status.state == "merged");
    merged && matches!(graph.push, Some(PushStatus::Tracked { unpushed: 0, .. }))
}

fn choice(can_delete: bool, note: impl Into<String>) -> BranchChoice {
    BranchChoice {
        can_delete,
        note: note.into(),
    }
}

/// The daemon never deletes a branch that is not merged, so the box is only a way to keep a merged
/// one. `graph` is None while the graph is read.
pub fn branch_choice(task: &FinishedTask, graph: Option<&GitGraph>) -> BranchChoice {
    let unknown = || choice(true, "Only when it is merged.");
    let Some(graph) = graph else {
        return unknown();
    };
    if graph.ahead == Some(0) {
        let base = graph.base.as_deref().unwrap_or("its base branch");
        return choice(true, format!("Merged into {base}."));
    }
    if let Some(pr) = &task.pr
        && task
            .pr_status
            .as_ref()
            .is_some_and(|status| status.state == "merged")
    {
        if branch_merged(task, graph) {
            let name = pull_request_name(pr);
            return choice(
                true,
                format!("{name} is merged, and origin has every commit."),
            );
        }
        let missing = unpushed(graph);
        return if missing > 0 {
            choice(
                false,
                format!("Kept: {} not on origin.", commits_are(missing)),
            )
        } else {
            choice(false, "Kept: origin does not have it.")
        };
    }
    if is_open_pull_request(task) {
        return choice(false, "Kept: the open pull request uses it.");
    }
    match graph.ahead {
        Some(ahead) if ahead > 0 => {
            choice(false, format!("Kept: {} not merged.", commits_are(ahead)))
        }
        _ => unknown(),
    }
}

/// A shell where an agent runs. The daemon gives a lane to any program in front of a shell, with
/// agent null for a program that is no agent.
pub fn is_agent_shell(shells: &TerminalList, name: &str) -> bool {
    shells.lanes.get(name).is_some_and(|lane| lane.is_agent())
}

/// The shells where a program that is no agent runs, such as a dev server, as (name, program).
/// Removing the workspace stops them.
pub fn busy_shells(shells: &TerminalList) -> Vec<(String, String)> {
    shells
        .terminals
        .iter()
        .filter_map(|name| {
            let program = shells.programs.get(name)?.as_deref()?;
            let busy = !is_agent_shell(shells, name) && !SHELL_PROGRAMS.contains(&program);
            busy.then(|| (name.clone(), program.to_string()))
        })
        .collect()
}

/// The toast after a workspace is removed.
pub fn removed_text(task_id: &str, branch: Option<&str>, branch_deleted: bool) -> String {
    let removed = format!("Removed the workspace of {task_id}");
    if branch_deleted {
        format!("{removed} and its merged branch.")
    } else if branch.is_none() {
        format!("{removed}.")
    } else {
        format!("{removed}. The branch is kept.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PR: &str = "https://github.com/o/r/pull/310";
    const PUSHED: PushStatus = PushStatus::Tracked {
        unpushed: 0,
        unpulled: 0,
    };

    fn status(state: &str) -> Option<PullRequestStatus> {
        Some(PullRequestStatus {
            state: state.into(),
            checks: "passing".into(),
        })
    }

    fn done() -> FinishedTask {
        FinishedTask {
            pr: None,
            pr_status: None,
            branch: Some("natamux/t-0058-icons".into()),
        }
    }

    fn with_pr(state: &str) -> FinishedTask {
        FinishedTask {
            pr: Some(PR.into()),
            pr_status: status(state),
            ..done()
        }
    }

    fn graph(ahead: u64, push: PushStatus) -> GitGraph {
        GitGraph {
            base: Some("main".into()),
            ahead: Some(ahead),
            push: Some(push),
            ..GitGraph::default()
        }
    }

    #[test]
    fn lets_a_merged_branch_go() {
        assert_eq!(
            branch_choice(&done(), Some(&graph(0, PUSHED))),
            choice(true, "Merged into main.")
        );
        assert_eq!(
            branch_choice(&with_pr("merged"), Some(&graph(3, PUSHED))),
            choice(
                true,
                "Pull request #310 is merged, and origin has every commit."
            )
        );
    }

    #[test]
    fn keeps_a_branch_with_work_that_is_not_merged_and_says_why() {
        assert_eq!(
            branch_choice(&with_pr("open"), Some(&graph(2, PUSHED))),
            choice(false, "Kept: the open pull request uses it.")
        );
        // A cancelled task is the same as `done()`: no pull request.
        assert_eq!(
            branch_choice(&done(), Some(&graph(2, PUSHED))),
            choice(false, "Kept: 2 commits are not merged.")
        );
        let unpushed = graph(
            3,
            PushStatus::Tracked {
                unpushed: 1,
                unpulled: 0,
            },
        );
        assert_eq!(
            branch_choice(&with_pr("merged"), Some(&unpushed)),
            choice(false, "Kept: 1 commit is not on origin.")
        );
    }

    #[test]
    fn leaves_the_choice_to_the_daemon_while_git_is_read() {
        assert_eq!(
            branch_choice(&done(), None),
            choice(true, "Only when it is merged.")
        );
    }

    #[test]
    fn says_origin_does_not_have_it_when_a_merged_branch_is_missing_there() {
        let g = graph(3, PushStatus::None);
        assert_eq!(
            branch_choice(&with_pr("merged"), Some(&g)),
            choice(false, "Kept: origin does not have it.")
        );
    }

    #[test]
    fn names_a_pull_request_with_no_number() {
        assert_eq!(pull_request_name("https://x/y"), "The pull request");
        assert_eq!(pull_request_name(PR), "Pull request #310");
    }

    fn shells(json: &str) -> TerminalList {
        serde_json::from_str(json).expect("a terminal list")
    }

    #[test]
    fn lists_the_shells_where_a_program_that_is_no_agent_runs() {
        let list = shells(
            r#"{"terminals":["1","2","3","4"],
            "lanes":{"1":{"stage":"implement","state":"running","agent":"claude-code"}},
            "programs":{"1":"claude","2":"zsh","3":"node","4":null}}"#,
        );
        assert_eq!(
            busy_shells(&list),
            vec![("3".to_string(), "node".to_string())]
        );
    }

    #[test]
    fn lists_a_program_that_has_a_lane_but_no_agent() {
        let list = shells(
            r#"{"terminals":["1","2"],
            "lanes":{"1":{"stage":"implement","state":"running","agent":"claude-code"},
                     "2":{"stage":null,"state":null,"agent":null}},
            "programs":{"1":"claude","2":"sleep"}}"#,
        );
        assert_eq!(
            busy_shells(&list),
            vec![("2".to_string(), "sleep".to_string())]
        );
    }

    #[test]
    fn says_whether_the_branch_went_too() {
        assert_eq!(
            removed_text("T-0058", Some("b"), true),
            "Removed the workspace of T-0058 and its merged branch."
        );
        assert_eq!(
            removed_text("T-0058", Some("b"), false),
            "Removed the workspace of T-0058. The branch is kept."
        );
        assert_eq!(
            removed_text("T-0058", None, false),
            "Removed the workspace of T-0058."
        );
    }
}
