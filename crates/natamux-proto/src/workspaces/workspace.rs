//! Workspaces: the routes of `packages/daemon/src/http/routes/workspaces.ts`, and the git types of
//! `packages/core/src/git/` that the Start a task and Remove workspace dialogs read.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::tasks::attachments::AttachmentUpload;
use crate::tasks::task::{Task, TaskContext};
use crate::workspaces::changes::ChangeStatus;

/// `base` of a new workspace: at most this many characters.
pub const MAX_BASE_LENGTH: usize = 255;
/// `prompt` of a new workspace: at most this many characters, after trimming.
pub const MAX_PROMPT_LENGTH: usize = 20_000;

/// `CHANGE_STATUSES` in `packages/core/src/git/changes.ts`: M modified, A added, D deleted,
/// R renamed, U not tracked yet, C conflict.
pub const CHANGE_STATUSES: [&str; 6] = ["M", "A", "D", "R", "U", "C"];

/// The body of `POST /workspaces`: a task, its worktree and its agent in one step. The answer
/// (202) comes before the worktree is made.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct NewWorkspaceInput {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// The branch of the new worktree; None: the project's pattern or the default name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<TaskContext>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentUpload>,
}

/// The body of `POST /tasks/:id/workspace`. The body may be left out when both fields are None.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CreateWorkspaceInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// `MAX_TRIES`: the most copies one fan out makes.
pub const MAX_TRIES: u32 = 5;

/// The body of `POST /tasks/:id/fan-out`: `count` is 1 to `MAX_TRIES`; with `start`, each copy gets a
/// workspace and Claude Code at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct FanOutInput {
    pub count: u32,
    pub start: bool,
}

/// The answer of `POST /tasks/:id/fan-out`: the new tries.
#[derive(Clone, Debug, Deserialize)]
pub struct FanOutAnswer {
    pub tasks: Vec<Task>,
}

/// The step a new workspace is at, while the daemon makes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreparingStep {
    Fetch,
    Checkout,
    /// A step of a later daemon.
    #[serde(other)]
    Unknown,
}

/// The body of `GET /workspaces/preparing`: task id to its step.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PreparingList {
    #[serde(default)]
    pub preparing: BTreeMap<String, PreparingStep>,
}

/// How much a workspace differs from where its branch started, uncommitted changes included.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct DiffStat {
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub removed: u64,
}

/// The body of `GET /diffstats`: task id to its stat.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct DiffStatList {
    #[serde(default)]
    pub stats: BTreeMap<String, DiffStat>,
}

/// One file that differs from the last commit. A line count is None when it is not known.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    pub path: String,
    pub status: ChangeStatus,
    #[serde(default)]
    pub original_path: Option<String>,
    #[serde(default)]
    pub added: Option<u64>,
    #[serde(default)]
    pub removed: Option<u64>,
}

/// The body of `GET /tasks/:id/changes`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChangeList {
    #[serde(default)]
    pub changes: Vec<ChangeEntry>,
}

/// Where the branch stands against `origin` (`pushStatusSchema`). `unpushed` counts the commits
/// that are on no branch of origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "remote", rename_all = "lowercase")]
pub enum PushStatus {
    /// The repository has no remote named origin.
    None,
    /// Origin has no branch of this name.
    New { unpushed: u64 },
    Tracked {
        unpushed: u64,
        #[serde(default)]
        unpulled: u64,
    },
    /// A kind of a later daemon.
    #[serde(other)]
    Unknown,
}

/// One commit of the graph (`graphCommitSchema`), newest first and each before its parents.
/// `refs` are branch and tag names; `on_branch` is false for a commit of the base branch only.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphCommit {
    pub hash: String,
    #[serde(default)]
    pub parents: Vec<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub on_branch: bool,
}

/// The history of a task branch (`gitGraphSchema`). `base` is the branch the main checkout is
/// on; None, and then `ahead` and `behind` are None too. `push` is None when HEAD is not on a
/// branch. `truncated` says the daemon left older commits out.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct GitGraph {
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub head: String,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub ahead: Option<u64>,
    #[serde(default)]
    pub behind: Option<u64>,
    #[serde(default)]
    pub push: Option<PushStatus>,
    #[serde(default)]
    pub commits: Vec<GraphCommit>,
    #[serde(default)]
    pub truncated: bool,
}

/// The answer of `DELETE /tasks/:id/workspace`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveWorkspaceAnswer {
    pub task: Task,
    #[serde(default)]
    pub branch_deleted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::task::TaskContext;

    const TASK: &str = r#"{"id":"T-1","title":"t","createdAt":"a","updatedAt":"b"}"#;

    #[test]
    fn a_workspace_for_a_task_sends_base_only_when_set() {
        let none = serde_json::to_string(&CreateWorkspaceInput::default()).unwrap();
        assert_eq!(none, "{}");
        let some = CreateWorkspaceInput {
            base: Some("dev".into()),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&some).unwrap(), r#"{"base":"dev"}"#);
        let both = CreateWorkspaceInput {
            base: Some("dev".into()),
            branch: Some("feature-x".into()),
        };
        let text = serde_json::to_string(&both).unwrap();
        assert_eq!(text, r#"{"base":"dev","branch":"feature-x"}"#);
    }

    #[test]
    fn a_fan_out_sends_the_count_and_start_and_reads_the_tries() {
        let input = FanOutInput {
            count: 2,
            start: false,
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"count":2,"start":false}"#
        );
        let answer: FanOutAnswer =
            serde_json::from_str(&format!(r#"{{"tasks":[{TASK}]}}"#)).unwrap();
        assert_eq!(answer.tasks[0].id, "T-1");
    }

    #[test]
    fn a_new_workspace_sends_only_what_is_set() {
        let input = NewWorkspaceInput {
            title: "Fix login".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"title":"Fix login"}"#
        );
    }

    #[test]
    fn a_full_new_workspace_keeps_the_field_order_and_the_ref_name() {
        let input = NewWorkspaceInput {
            title: "Fix".into(),
            repo: Some("natamux".into()),
            base: Some("dev".into()),
            branch: Some("ABC-12".into()),
            agent: Some("claude-code".into()),
            prompt: Some("Do it".into()),
            context: Some(TaskContext {
                source: "github-issue".into(),
                reference: "42".into(),
                ..Default::default()
            }),
            attachments: vec![AttachmentUpload {
                name: "a.png".into(),
                data: "AAAA".into(),
            }],
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            concat!(
                r#"{"title":"Fix","repo":"natamux","base":"dev","branch":"ABC-12","#,
                r#""agent":"claude-code","#,
                r#""prompt":"Do it","context":{"source":"github-issue","ref":"42"},"#,
                r#""attachments":[{"name":"a.png","data":"AAAA"}]}"#
            )
        );
    }

    #[test]
    fn reads_the_preparing_steps_and_a_step_from_a_later_daemon() {
        let list: PreparingList =
            serde_json::from_str(r#"{"preparing":{"T-1":"fetch","T-2":"checkout","T-3":"zip"}}"#)
                .unwrap();
        assert_eq!(list.preparing["T-1"], PreparingStep::Fetch);
        assert_eq!(list.preparing["T-2"], PreparingStep::Checkout);
        assert_eq!(list.preparing["T-3"], PreparingStep::Unknown);
        let none: PreparingList = serde_json::from_str(r#"{"preparing":{}}"#).unwrap();
        assert!(none.preparing.is_empty());
    }

    #[test]
    fn reads_diff_stats() {
        let list: DiffStatList = serde_json::from_str(
            r#"{"stats":{"T-1":{"files":0,"added":0,"removed":0},"T-2":{"files":3,"added":10,"removed":2}}}"#,
        )
        .unwrap();
        assert_eq!(list.stats["T-1"], DiffStat::default());
        assert_eq!(list.stats["T-2"].added, 10);
    }

    #[test]
    fn reads_changes_with_and_without_counts() {
        let list: ChangeList = serde_json::from_str(
            r#"{"changes":[
                {"path":"a.ts","status":"M","originalPath":null,"added":3,"removed":1},
                {"path":"b.png","status":"U","originalPath":null,"added":null,"removed":null},
                {"path":"c.ts","status":"R","originalPath":"d.ts"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(list.changes.len(), 3);
        assert_eq!(list.changes[0].added, Some(3));
        assert_eq!(list.changes[1].added, None);
        assert_eq!(list.changes[2].original_path.as_deref(), Some("d.ts"));
        assert_eq!(list.changes[1].status, ChangeStatus::Untracked);
    }

    #[test]
    fn reads_a_graph_for_each_push_status() {
        let graph = |push: &str| -> GitGraph {
            serde_json::from_str(&format!(
                r#"{{"branch":"b","head":"abc","base":"main","ahead":2,"behind":0,
                    "push":{push},"commits":[{{"hash":"abc","parents":[],"author":"a",
                    "date":"d","subject":"s","refs":[],"onBranch":true}}],"truncated":false}}"#
            ))
            .unwrap()
        };
        assert_eq!(graph(r#"{"remote":"none"}"#).push, Some(PushStatus::None));
        let commit = &graph("null").commits[0];
        assert_eq!((commit.hash.as_str(), commit.on_branch), ("abc", true));
        assert_eq!(
            graph(r#"{"remote":"new","unpushed":2}"#).push,
            Some(PushStatus::New { unpushed: 2 })
        );
        assert_eq!(
            graph(r#"{"remote":"tracked","unpushed":1,"unpulled":4}"#).push,
            Some(PushStatus::Tracked {
                unpushed: 1,
                unpulled: 4
            })
        );
        assert_eq!(
            graph(r#"{"remote":"later"}"#).push,
            Some(PushStatus::Unknown)
        );
        assert_eq!(graph("null").push, None);
        assert_eq!(graph("null").ahead, Some(2));
    }

    #[test]
    fn a_graph_of_a_detached_checkout_has_no_base_or_counts() {
        let graph: GitGraph = serde_json::from_str(
            r#"{"branch":null,"head":"abc","base":null,"ahead":null,"behind":null,"push":null,
                "commits":[],"truncated":false}"#,
        )
        .unwrap();
        assert_eq!(graph.base, None);
        assert_eq!(graph.ahead, None);
    }

    #[test]
    fn a_graph_from_an_older_daemon_has_no_push() {
        let graph: GitGraph = serde_json::from_str(
            r#"{"branch":"b","head":"abc","base":"main","ahead":0,"behind":0}"#,
        )
        .unwrap();
        assert_eq!(graph.push, None);
    }

    #[test]
    fn reads_the_remove_answer() {
        let answer: RemoveWorkspaceAnswer =
            serde_json::from_str(&format!(r#"{{"task":{TASK},"branchDeleted":true}}"#)).unwrap();
        assert_eq!(answer.task.id, "T-1");
        assert!(answer.branch_deleted);
        let kept: RemoveWorkspaceAnswer =
            serde_json::from_str(&format!(r#"{{"task":{TASK},"branchDeleted":false}}"#)).unwrap();
        assert!(!kept.branch_deleted);
    }
}
