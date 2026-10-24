//! The fake's projects and GitHub issues of screen 7: Add project with the daemon's path rules,
//! three open issues of `natamux`, and the `gh_missing` switch of the reference state.

use natamux_client::ClientError;
use natamux_proto::projects::issues::GitHubIssue;
use natamux_proto::projects::repo::{AddRepo, Repo};
use natamux_proto::tasks::task::Task;

use super::{Answer, Data, FakeDaemon};

const GH_MISSING: &str = "GitHub CLI failed: gh is not installed or not on the PATH";
const TITLE_LIMIT: usize = 200;

#[derive(Default)]
pub(super) struct ProjectsState {
    issues: Vec<GitHubIssue>,
    gh_missing: bool,
}

fn issue(number: u64, title: &str, labels: &[&str], task_id: Option<&str>) -> GitHubIssue {
    GitHubIssue {
        number,
        title: title.into(),
        body: format!("Details of issue {number}."),
        url: format!("https://github.com/natamux/natamux/issues/{number}"),
        labels: labels.iter().map(|label| (*label).into()).collect(),
        task_id: task_id.map(Into::into),
    }
}

impl ProjectsState {
    pub(super) fn seeded() -> ProjectsState {
        ProjectsState {
            issues: vec![
                issue(12, "Show stock levels on product pages", &["feature"], None),
                issue(15, "Coupon total goes negative", &["bug"], Some("T-0141")),
                issue(18, "Speed up the catalog search", &[], None),
            ],
            gh_missing: false,
        }
    }
}

fn bad_request(message: String) -> ClientError {
    ClientError::Status {
        status: 400,
        message,
    }
}

/// The daemon's `slugify`: lower case words joined by `-`, at most 40 characters.
fn slugify(text: &str) -> String {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let slug: String = words.join("-").chars().take(40).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.into()
    }
}

fn folder_name(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

impl Data {
    /// The daemon's own worktree folder, when the seed has none.
    pub(super) fn add_projects_seed(&mut self) {
        if self.worktrees_dir.is_none() {
            self.worktrees_dir = Some("/data/worktrees".into());
        }
    }

    fn add_repo(&mut self, input: &AddRepo) -> Answer<Repo> {
        let path = input.path.trim_end_matches('/');
        if !path.starts_with('/') {
            return Err(bad_request(format!(
                "{} is not an absolute path to a folder",
                input.path
            )));
        }
        if self.repos.iter().any(|repo| repo.path == path) {
            return Err(bad_request(format!("{path} is added already")));
        }
        let worktrees_dir = input.worktrees_dir.as_deref().map(str::trim);
        if let Some(dir) = worktrees_dir.filter(|dir| !dir.starts_with('/')) {
            return Err(bad_request(format!(
                "The worktree folder {dir} is not an absolute path"
            )));
        }
        let name = input
            .name
            .clone()
            .unwrap_or_else(|| folder_name(path).into());
        let base = slugify(&name);
        let mut id = base.clone();
        let mut number = 2;
        while self.repos.iter().any(|repo| repo.id == id) {
            id = format!("{base}-{number}");
            number += 1;
        }
        let repo = Repo {
            id,
            name,
            path: path.into(),
            worktrees_dir: worktrees_dir.map(Into::into),
        };
        self.repos.push(repo.clone());
        let event = self.repos_event();
        self.send(event);
        Ok(repo)
    }

    fn gh(&self) -> Answer<()> {
        if self.projects.gh_missing {
            return Err(ClientError::Status {
                status: 409,
                message: GH_MISSING.into(),
            });
        }
        Ok(())
    }

    /// Only the `natamux` project has issues; any other lists none.
    fn import_issues(&mut self, numbers: &[u64], repo: Option<&str>) -> Answer<Vec<Task>> {
        self.gh()?;
        let wanted: Vec<GitHubIssue> = if repo.is_none_or(|id| id == "natamux") {
            self.projects
                .issues
                .iter()
                .filter(|item| numbers.contains(&item.number) && item.task_id.is_none())
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let mut made = Vec::new();
        for item in wanted {
            let title = item.title.trim();
            let title = if title.is_empty() {
                format!("Issue #{}", item.number)
            } else {
                title.chars().take(TITLE_LIMIT).collect()
            };
            let mut task = self.new_task(&title, repo.map(Into::into));
            let body = item.body.trim();
            let line = format!("Imported from {}", item.url);
            task.description = if body.is_empty() {
                line
            } else {
                format!("{body}\n\n{line}")
            };
            task.labels = item.labels.clone();
            self.tasks.insert(0, task.clone());
            if let Some(known) = self
                .projects
                .issues
                .iter_mut()
                .find(|known| known.number == item.number)
            {
                known.task_id = Some(task.id.clone());
            }
            made.push(task);
        }
        self.tasks_changed();
        Ok(made)
    }
}

impl FakeDaemon {
    pub fn add_repo(&self, input: &AddRepo) -> Answer<Repo> {
        self.data().add_repo(input)
    }

    pub fn list_issues(&self, repo: Option<&str>) -> Answer<Vec<GitHubIssue>> {
        let data = self.data();
        data.gh()?;
        let own = repo.is_none_or(|id| id == "natamux");
        Ok(if own {
            data.projects.issues.clone()
        } else {
            Vec::new()
        })
    }

    pub fn import_issues(&self, numbers: &[u64], repo: Option<&str>) -> Answer<Vec<Task>> {
        self.data().import_issues(numbers, repo)
    }

    /// For the reference shot: the GitHub CLI is not installed.
    pub fn set_gh_missing(&self, missing: bool) {
        self.data().projects.gh_missing = missing;
    }
}

#[cfg(test)]
#[path = "fake_projects_tests.rs"]
mod tests;
