//! `issue-linker.ts`: opens a GitHub issue for a task made in the app, when its project asks for
//! one (`tracker.createIssue`), and links the pull request to it.
//!
//! `create` blocks, so a request on the spec approval runs it on a thread of its own, and a second
//! `create` for a task waits for the first one instead of opening a second issue.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};

use natamux_core::js::{is_js_space, js_trim};
use natamux_git::gh::{GitHubError, RunGh};
use natamux_proto::projects::repo::TrackerSettings;
use natamux_proto::tasks::task::{IssueLink, Task};
use natamux_stages::files::read_stage_file;
use serde_json::{Value, json};

use crate::github::issue_importer::first_units;
use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::jobs::Jobs;
use crate::stages::run::{RepoPath, SettingsOf};
use crate::tasks::errors::TaskError;
use crate::tasks::service::TaskService;

const MAX_BODY_CHARS: usize = 60_000;

/// `IssueLinkError`: a task that is linked already, a project without a tracker, a race.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueLinkError(pub String);

impl std::fmt::Display for IssueLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IssueLinkError {}

#[derive(Debug)]
pub enum LinkError {
    Link(IssueLinkError),
    GitHub(GitHubError),
    Task(TaskError),
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkError::Link(error) => write!(f, "{error}"),
            LinkError::GitHub(error) => write!(f, "{error}"),
            LinkError::Task(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for LinkError {}

impl From<IssueLinkError> for LinkError {
    fn from(error: IssueLinkError) -> LinkError {
        LinkError::Link(error)
    }
}

impl From<GitHubError> for LinkError {
    fn from(error: GitHubError) -> LinkError {
        LinkError::GitHub(error)
    }
}

impl From<TaskError> for LinkError {
    fn from(error: TaskError) -> LinkError {
        LinkError::Task(error)
    }
}

impl LinkError {
    /// `String(error)` of the Node error: its name, then its message.
    pub fn log_text(&self) -> String {
        match self {
            LinkError::Link(error) => format!("IssueLinkError: {error}"),
            LinkError::GitHub(error) => format!("GitHubError: {error}"),
            LinkError::Task(error) => error.to_string(),
        }
    }

    /// The same error for a request that waited for another one. `TaskError` cannot be cloned: a
    /// missing task stays one, any other task error keeps its text.
    fn again(&self) -> LinkError {
        match self {
            LinkError::Link(error) => LinkError::Link(error.clone()),
            LinkError::GitHub(error) => LinkError::GitHub(error.clone()),
            LinkError::Task(TaskError::NotFound(id)) => {
                LinkError::Task(TaskError::NotFound(id.clone()))
            }
            LinkError::Task(error) => LinkError::Task(TaskError::Failed(error.to_string())),
        }
    }
}

/// `/https:\/\/\S+\/issues\/\d+/`: the issue address `gh issue create` prints.
pub fn issue_url(output: &str) -> Option<String> {
    let marker = "/issues/";
    for (start, _) in output.match_indices("https://") {
        let rest = &output[start..];
        let run = &rest[..rest.find(is_js_space).unwrap_or(rest.len())];
        // `\S+` is greedy: the last `/issues/<digits>` of the run wins, after one character.
        let first = "https://".len() + 1;
        let end = run.rmatch_indices(marker).find_map(|(at, _)| {
            let after = at + marker.len();
            let digits = run[after..].bytes().take_while(u8::is_ascii_digit).count();
            (at >= first && digits > 0).then_some(after + digits)
        });
        if let Some(end) = end {
            return Some(run[..end].to_string());
        }
    }
    None
}

/// The digits after the first `/issues/` that has some.
fn issue_number(url: &str) -> &str {
    url.match_indices("/issues/")
        .map(|(at, found)| {
            let rest = &url[at + found.len()..];
            &rest[..rest.bytes().take_while(u8::is_ascii_digit).count()]
        })
        .find(|digits| !digits.is_empty())
        .unwrap_or("")
}

/// The line a pull request body ends with, so the tracker closes the item on merge.
pub fn closing_line(issue: &IssueLink) -> String {
    if issue.provider == "github" {
        format!("Closes {}", issue.key)
    } else {
        format!("Tracked in {}: {}", issue.key, issue.url)
    }
}

pub struct IssueLinkerOptions {
    pub tasks: TaskService,
    pub bus: Arc<EventBus>,
    pub data_dir: PathBuf,
    pub repo_path: RepoPath,
    /// The settings of a repository, read at each call, for `tracker` and the stages.
    pub settings_of: SettingsOf,
    pub gh: RunGh,
    pub jobs: Arc<Jobs>,
}

/// An issue being opened: the requests for the same task wait on it.
#[derive(Default)]
struct Flight {
    done: Mutex<Option<Result<Task, LinkError>>>,
    ready: Condvar,
}

/// Ends the flight of a task: removes it from the open ones, and answers the waiting requests
/// when the leader did not (it panicked).
struct FlightEnd<'a> {
    inner: &'a Inner,
    task_id: &'a str,
    flight: &'a Flight,
}

impl Drop for FlightEnd<'_> {
    fn drop(&mut self) {
        locked(&self.inner.creating).remove(self.task_id);
        let mut done = locked(&self.flight.done);
        if done.is_none() {
            *done = Some(Err(IssueLinkError("The request ended".into()).into()));
        }
        drop(done);
        self.flight.ready.notify_all();
    }
}

struct Inner {
    options: IssueLinkerOptions,
    creating: Mutex<HashMap<String, Arc<Flight>>>,
    /// Threads that open an issue after a spec approval.
    pending: Mutex<usize>,
    idle: Condvar,
    subscription: Mutex<Option<u64>>,
}

pub struct IssueLinker {
    inner: Arc<Inner>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl IssueLinker {
    pub fn new(options: IssueLinkerOptions) -> IssueLinker {
        IssueLinker {
            inner: Arc::new(Inner {
                options,
                creating: Mutex::new(HashMap::new()),
                pending: Mutex::new(0),
                idle: Condvar::new(),
                subscription: Mutex::new(None),
            }),
        }
    }

    pub fn start(&self) {
        let mut subscription = locked(&self.inner.subscription);
        if subscription.is_some() {
            return;
        }
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        *subscription = Some(self.inner.options.bus.subscribe(move |message| {
            if let Some(inner) = weak.upgrade() {
                inner.on_message(message);
            }
        }));
    }

    pub fn stop(&self) {
        if let Some(id) = locked(&self.inner.subscription).take() {
            self.inner.options.bus.unsubscribe(id);
        }
    }

    /// Waits for the issues being opened in the background. For tests.
    pub fn settle(&self) {
        let mut pending = locked(&self.inner.pending);
        while *pending > 0 {
            pending = self
                .inner
                .idle
                .wait(pending)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Opens the issue now: its title is the task's, its body the spec (else the notes). A request
    /// that comes while another opens the issue for the same task gets that one's answer.
    pub fn create(&self, task_id: &str) -> Result<Task, LinkError> {
        self.inner.create(task_id)
    }

    /// The body of the task's pull request. Opens the issue first when the project wants one by
    /// now: `on-pr`, or `on-spec-approved` in a pipeline without a spec stage.
    pub fn pull_request_body(&self, task: &Task) -> Result<String, LinkError> {
        let inner = &self.inner;
        let tracker = inner.tracker(task.repo.as_deref());
        let wants_issue = tracker.kind == "github"
            && (tracker.create_issue == "on-pr"
                || (tracker.create_issue == "on-spec-approved" && !inner.has_spec_stage(task)));
        let current = if wants_issue && task.issue.is_none() {
            inner.create(&task.id)?
        } else {
            task.clone()
        };
        let notes = js_trim(&current.description);
        let body = if notes.is_empty() {
            format!("Task {} in Natamux.", current.id)
        } else {
            notes.to_string()
        };
        Ok(match &current.issue {
            Some(issue) if tracker.link_in_pr => format!("{body}\n\n{}", closing_line(issue)),
            _ => body,
        })
    }
}

impl Inner {
    fn tracker(&self, repo: Option<&str>) -> TrackerSettings {
        (self.options.settings_of)(repo).tracker
    }

    fn has_spec_stage(&self, task: &Task) -> bool {
        self.options.tasks.steps_of(task).is_ok_and(|steps| {
            steps
                .iter()
                .any(|step| step.stages().iter().any(|stage| stage.id == "spec"))
        })
    }

    fn on_message(self: &Arc<Inner>, message: &DaemonMessage) {
        let DaemonMessage::TaskUpdated {
            task,
            event,
            accepted: true,
            ..
        } = message
        else {
            return;
        };
        let spec = event.payload.get("stage").and_then(Value::as_str) == Some("spec");
        if event.kind != "stage.approved" || !spec {
            return;
        }
        let tracker = self.tracker(task.repo.as_deref());
        if tracker.kind == "github"
            && tracker.create_issue == "on-spec-approved"
            && task.issue.is_none()
        {
            self.open_in_background(&task.id);
        }
    }

    // A failure leaves the task without an issue; "Create issue" can try again.
    fn open_in_background(self: &Arc<Inner>, task_id: &str) {
        *locked(&self.pending) += 1;
        let inner = Arc::clone(self);
        let id = task_id.to_string();
        let spawned = std::thread::Builder::new()
            .name("issue-link".into())
            .spawn(move || {
                if let Err(error) = inner.create(&id) {
                    eprintln!("Could not open an issue for {id}: {}", error.log_text());
                }
                inner.finish_one();
            });
        if let Err(error) = spawned {
            eprintln!("Could not open an issue for {task_id}: {error}");
            self.finish_one();
        }
    }

    fn finish_one(&self) {
        let mut pending = locked(&self.pending);
        *pending = pending.saturating_sub(1);
        self.idle.notify_all();
    }

    fn create(&self, task_id: &str) -> Result<Task, LinkError> {
        let (flight, leader) = {
            let mut creating = locked(&self.creating);
            match creating.get(task_id) {
                Some(running) => (Arc::clone(running), false),
                None => {
                    let flight = Arc::new(Flight::default());
                    creating.insert(task_id.to_string(), Arc::clone(&flight));
                    (flight, true)
                }
            }
        };
        if !leader {
            let mut done = locked(&flight.done);
            while done.is_none() {
                done = flight
                    .ready
                    .wait(done)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            return match done.as_ref() {
                Some(Ok(task)) => Ok(task.clone()),
                Some(Err(error)) => Err(error.again()),
                None => Err(IssueLinkError("The request ended".into()).into()),
            };
        }
        // Resolves the flight also when `create_now` panics, so the requests that wait do not hang.
        let _flight = FlightEnd {
            inner: self,
            task_id,
            flight: &flight,
        };
        let result =
            self.options
                .jobs
                .run_blocking("issue", "Opening GitHub issue", Some(task_id), || {
                    self.create_now(task_id)
                });
        let copy = match &result {
            Ok(task) => Ok(task.clone()),
            Err(error) => Err(error.again()),
        };
        *locked(&flight.done) = Some(copy);
        flight.ready.notify_all();
        result
    }

    fn create_now(&self, task_id: &str) -> Result<Task, LinkError> {
        let options = &self.options;
        let task = options.tasks.get_task(task_id)?;
        if let Some(issue) = &task.issue {
            let text = format!("{task_id} is linked to {} already", issue.key);
            return Err(IssueLinkError(text).into());
        }
        if self.tracker(task.repo.as_deref()).kind != "github" {
            let text = "This project has no issue tracker. Set tracker.type in .natamux.json";
            return Err(IssueLinkError(text.into()).into());
        }
        let workspace = task.workspace.as_deref();
        let spec = read_stage_file(&options.data_dir, &task.id, workspace, "spec.md")
            .ok()
            .flatten();
        let text = js_trim(spec.as_deref().unwrap_or(&task.description));
        let text = if text.is_empty() {
            task.title.as_str()
        } else {
            text
        };
        let body = format!("{text}\n\nMade by Natamux for {}.", task.id);
        let args = [
            "issue".to_string(),
            "create".into(),
            "--title".into(),
            task.title.clone(),
            "--body".into(),
            first_units(&body, MAX_BODY_CHARS),
        ];
        let folder = (options.repo_path)(task.repo.as_deref())?;
        let output = (options.gh)(Path::new(&folder), &args)?;
        let Some(url) = issue_url(&output) else {
            let text = format!("no issue address in the answer: {}", js_trim(&output));
            return Err(GitHubError::new(&text).into());
        };
        let number = issue_number(&url);
        // The task may have been linked while gh ran, for example from its context.
        if let Some(current) = options.tasks.get_task(task_id)?.issue {
            let payload = json!({ "url": url });
            options
                .tasks
                .record_event(&task.id, "issue.created", payload, "system")?;
            let text = format!(
                "{task_id} was linked to {} while {url} was opened. Close one of them",
                current.key
            );
            return Err(IssueLinkError(text).into());
        }
        let link = json!({ "provider": "github", "key": format!("#{number}"), "url": url });
        let linked = options.tasks.link_issue(&task.id, &link, "system")?;
        let payload = json!({ "url": url });
        options
            .tasks
            .record_event(&task.id, "issue.created", payload, "system")?;
        Ok(linked)
    }
}
