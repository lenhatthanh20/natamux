//! GitHub issues the daemon lists and imports (`packages/core/src/projects/github.ts`).

use serde::{Deserialize, Serialize};

use crate::tasks::task::Task;

/// An open issue of the project's repository. `task_id`: the task made from it already.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubIssue {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub url: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub task_id: Option<String>,
}

/// The answer of `GET /github/issues`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct IssueList {
    #[serde(default)]
    pub issues: Vec<GitHubIssue>,
}

/// The body of `POST /github/issues/import`. `repo` is left out for the default project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImportIssues {
    pub numbers: Vec<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// The answer of `POST /github/issues/import`: one task per issue.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct ImportedTasks {
    #[serde(default)]
    pub tasks: Vec<Task>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_issue_list_with_the_task_of_an_imported_issue() {
        let list: IssueList = serde_json::from_str(
            r#"{"issues":[{"number":12,"title":"Crash","body":"Steps","url":"https://github.com/o/r/issues/12","labels":["bug","ui"],"taskId":"T-0007"},
                {"number":13,"title":"Idea","body":"","url":"https://github.com/o/r/issues/13","labels":[],"taskId":null}]}"#,
        )
        .unwrap();
        assert_eq!(list.issues[0].number, 12);
        assert_eq!(list.issues[0].labels, ["bug", "ui"]);
        assert_eq!(list.issues[0].task_id.as_deref(), Some("T-0007"));
        assert_eq!(list.issues[1].task_id, None);
    }

    #[test]
    fn the_import_body_leaves_out_the_default_project() {
        let body = ImportIssues {
            numbers: vec![3, 5],
            repo: None,
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"numbers":[3,5]}"#
        );
        let body = ImportIssues {
            numbers: vec![3],
            repo: Some("my-app".into()),
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            r#"{"numbers":[3],"repo":"my-app"}"#
        );
    }

    #[test]
    fn reads_the_imported_tasks() {
        let list: ImportedTasks = serde_json::from_str(
            r#"{"tasks":[{"id":"T-0009","title":"Crash","updatedAt":"2026-10-22T00:00:00.000Z"}]}"#,
        )
        .unwrap();
        assert_eq!(list.tasks[0].id, "T-0009");
    }
}
