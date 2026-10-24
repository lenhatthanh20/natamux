//! `issue-importer.ts`: turns open GitHub issues into draft tasks. An issue becomes a task once.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use natamux_core::js::{is_js_space, js_trim};
use natamux_git::gh::{GitHubError, RunGh};
use natamux_proto::projects::issues::GitHubIssue;
use natamux_proto::tasks::task::Task;
use serde_json::{Value, json};

use crate::shared::jobs::Jobs;
use crate::stages::run::RepoPath;
use crate::tasks::errors::TaskError;
use crate::tasks::service::TaskService;

pub const MAX_ISSUES: usize = 100;
const TITLE_LIMIT: usize = 200;

#[derive(Debug)]
pub enum IssueImportError {
    GitHub(GitHubError),
    Task(TaskError),
}

impl std::fmt::Display for IssueImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueImportError::GitHub(error) => write!(f, "{error}"),
            IssueImportError::Task(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for IssueImportError {}

impl From<GitHubError> for IssueImportError {
    fn from(error: GitHubError) -> IssueImportError {
        IssueImportError::GitHub(error)
    }
}

impl From<TaskError> for IssueImportError {
    fn from(error: TaskError) -> IssueImportError {
        IssueImportError::Task(error)
    }
}

/// `text.slice(0, units)`: JavaScript counts UTF-16 units.
pub(crate) fn first_units(text: &str, units: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|each| {
            used += each.len_utf16();
            used <= units
        })
        .collect()
}

/// `importedFromLine`.
pub fn imported_from_line(url: &str) -> String {
    format!("Imported from {url}")
}

/// Reads `gh issue list --json number,title,body,url,labels`. `task_id` stays None.
pub fn parse_issue_list(output: &str) -> Result<Vec<GitHubIssue>, GitHubError> {
    let items = match serde_json::from_str::<Value>(output) {
        Ok(Value::Array(items)) => items,
        _ => {
            return Err(GitHubError::new(&format!(
                "could not read the issue list: {}",
                first_units(output, 200)
            )));
        }
    };
    Ok(items.iter().filter_map(issue_of).collect())
}

fn issue_of(item: &Value) -> Option<GitHubIssue> {
    let number = item.get("number")?.as_u64()?;
    let url = item.get("url")?.as_str().filter(|url| !url.is_empty())?;
    let text = |name: &str| item.get(name).and_then(Value::as_str).unwrap_or("");
    let labels = item
        .get("labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label.get("name")?.as_str())
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    Some(GitHubIssue {
        number,
        title: text("title").into(),
        body: text("body").into(),
        url: url.into(),
        labels,
        task_id: None,
    })
}

/// The address in the last line of a task's notes, `/Imported from (\S+)\s*$/`.
fn imported_url(description: &str) -> Option<&str> {
    let end = description.trim_end_matches(is_js_space);
    let start = end.rfind(is_js_space).map_or(0, |at| {
        at + end[at..].chars().next().map_or(1, char::len_utf8)
    });
    let url = &end[start..];
    (!url.is_empty() && end[..start].ends_with("Imported from ")).then_some(url)
}

pub struct IssueImporterOptions {
    pub tasks: TaskService,
    /// The folder of a repository (None: the default one), so gh knows which repository to ask.
    pub repo_path: RepoPath,
    pub gh: RunGh,
    pub jobs: Arc<Jobs>,
}

pub struct IssueImporter {
    tasks: TaskService,
    repo_path: RepoPath,
    gh: RunGh,
    jobs: Arc<Jobs>,
}

impl IssueImporter {
    pub fn new(options: IssueImporterOptions) -> IssueImporter {
        IssueImporter {
            tasks: options.tasks,
            repo_path: options.repo_path,
            gh: options.gh,
            jobs: options.jobs,
        }
    }

    pub fn list(&self, repo: Option<&str>) -> Result<Vec<GitHubIssue>, IssueImportError> {
        let folder = (self.repo_path)(repo)?;
        let limit = MAX_ISSUES.to_string();
        let args = [
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            limit.as_str(),
            "--json",
            "number,title,body,url,labels",
        ]
        .map(str::to_string);
        let output = (self.gh)(Path::new(&folder), &args)?;
        let imported = self.imported_task_ids()?;
        Ok(parse_issue_list(&output)?
            .into_iter()
            .map(|issue| GitHubIssue {
                task_id: imported.get(&issue.url).cloned(),
                ..issue
            })
            .collect())
    }

    /// Makes a task for each issue number that is open and not imported yet. Stops at the first
    /// issue that cannot be made; the tasks before it stay.
    pub fn import(
        &self,
        numbers: &[u64],
        repo: Option<&str>,
    ) -> Result<Vec<Task>, IssueImportError> {
        self.jobs
            .run_blocking("issue-import", "Importing issues", None, || {
                self.import_now(numbers, repo)
            })
    }

    fn import_now(
        &self,
        numbers: &[u64],
        repo: Option<&str>,
    ) -> Result<Vec<Task>, IssueImportError> {
        let issues = self
            .list(repo)?
            .into_iter()
            .filter(|issue| numbers.contains(&issue.number) && issue.task_id.is_none());
        let mut made = Vec::new();
        for issue in issues {
            let title = match js_trim(&issue.title) {
                "" => format!("Issue #{}", issue.number),
                title => title.to_string(),
            };
            let notes = [
                js_trim(&issue.body).to_string(),
                imported_from_line(&issue.url),
            ];
            let notes: Vec<String> = notes.into_iter().filter(|part| !part.is_empty()).collect();
            let input = json!({
                "title": first_units(&title, TITLE_LIMIT),
                "description": notes.join("\n\n"),
                "labels": issue.labels,
                "repo": repo,
            });
            made.push(self.tasks.create_task(Some(&input), &[], true)?);
        }
        Ok(made)
    }

    /// Issue address to task id, read from the last line of each task's notes.
    fn imported_task_ids(&self) -> Result<HashMap<String, String>, TaskError> {
        let mut found = HashMap::new();
        for task in self.tasks.list_tasks(None)? {
            if let Some(url) = imported_url(&task.description) {
                found.insert(url.to_string(), task.id);
            }
        }
        Ok(found)
    }
}
