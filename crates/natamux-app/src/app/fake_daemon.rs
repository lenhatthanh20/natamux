//! `--fake-daemon`: the data of every screen-1 state, so CI can take screenshots with no daemon.
//! Writes succeed and change the data, and each open feed gets the changed part, so dialogs and
//! menus can be shot and tested. The data is `fake_dashboard.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use natamux_client::{ClientError, DashboardEvent, LinkStatus, SendEventAnswer};
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::agents::start::StartAgentInput;
use natamux_proto::events::kind::PreparingMessage;
use natamux_proto::events::task_event::{StoredEvent, TaskEventInput};
use natamux_proto::projects::repo::{BranchList, Repo, SkillList};
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::{
    DeleteTaskAnswer, NewTaskInput, Task, TaskState, TemplateTaskInput,
};
use natamux_proto::tasks::template::TaskTemplate;
use natamux_proto::terminals::ended::EndedShell;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{
    ChangeEntry, DiffStat, GitGraph, NewWorkspaceInput, PreparingStep, RemoveWorkspaceAnswer,
};
use serde_json::Value;

const SEED: &str = include_str!("fake_dashboard.json");
/// The time of every fake write, so screenshots do not change with the clock.
pub const NOW: &str = "2026-10-22T09:31:00.000Z";
/// Pinned in the view state of the shots.
pub const PINNED: &[&str] = &["T-0141"];

#[derive(Default)]
struct Data {
    tasks: Vec<Task>,
    repos: Vec<Repo>,
    worktrees_dir: Option<String>,
    jobs: Vec<Job>,
    activity: BTreeMap<String, TaskActivity>,
    usage: BTreeMap<String, TaskUsage>,
    diff_stats: BTreeMap<String, DiffStat>,
    terminals: BTreeMap<String, TerminalList>,
    events: BTreeMap<String, Vec<StoredEvent>>,
    stage_files: BTreeMap<String, Vec<natamux_proto::stages::actions::StageFile>>,
    templates: Vec<TaskTemplate>,
    branches: BranchList,
    skills: SkillList,
    changes: BTreeMap<String, Vec<ChangeEntry>>,
    graphs: BTreeMap<String, GitGraph>,
    preparing: BTreeMap<String, PreparingStep>,
    ended: BTreeMap<String, Vec<EndedShell>>,
    git: git::GitState,
    settings: settings::SettingsFiles,
    next_number: u32,
    feeds: Vec<UnboundedSender<DashboardEvent>>,
    link: FakeLink,
}

/// How the fake's feed behaves: the `skeleton` and `load-error` shots need one that never loads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FakeLink {
    #[default]
    Live,
    /// The task list never comes.
    Stalled,
    /// The daemon cannot be reached, and the feed keeps trying.
    Down,
}

/// One part of the seed, or its default when it is missing or cannot be read
/// (`every_part_of_the_seed_reads` checks none is).
macro_rules! part {
    ($seed:expr, $name:literal) => {
        serde_json::from_value($seed.get($name).cloned().unwrap_or(Value::Null)).unwrap_or_default()
    };
}

impl Data {
    fn seed() -> Data {
        let seed: Value = serde_json::from_str(SEED).unwrap_or(Value::Null);
        let mut data = Data {
            tasks: part!(seed, "tasks"),
            repos: part!(seed, "repos"),
            worktrees_dir: part!(seed, "worktreesDir"),
            jobs: part!(seed, "jobs"),
            activity: part!(seed, "activity"),
            usage: part!(seed, "usage"),
            diff_stats: part!(seed, "diffStats"),
            terminals: part!(seed, "terminals"),
            events: part!(seed, "events"),
            stage_files: BTreeMap::new(),
            templates: part!(seed, "templates"),
            branches: part!(seed, "branches"),
            skills: part!(seed, "skills"),
            changes: part!(seed, "changes"),
            graphs: part!(seed, "graphs"),
            preparing: BTreeMap::new(),
            ended: part!(seed, "endedShells"),
            git: git::GitState::default(),
            settings: settings::SettingsFiles::default(),
            next_number: 200,
            feeds: Vec::new(),
            link: FakeLink::Live,
        };
        data.add_stage_seed();
        data.add_git_seed();
        data.add_settings_seed();
        data
    }

    fn everything(&self) -> Vec<DashboardEvent> {
        match self.link {
            FakeLink::Live => {}
            FakeLink::Stalled => return Vec::new(),
            FakeLink::Down => {
                let error = ClientError::Unreachable {
                    url: "http://127.0.0.1:4317".into(),
                    reason: "Connection refused (os error 61)".into(),
                };
                return vec![DashboardEvent::Status(LinkStatus::Reconnecting(error))];
            }
        }
        vec![
            DashboardEvent::Status(LinkStatus::Live),
            DashboardEvent::Tasks(self.tasks.clone()),
            self.repos_event(),
            DashboardEvent::Jobs(self.jobs.clone()),
            DashboardEvent::Activity(self.activity.clone()),
            DashboardEvent::Usage(self.usage.clone()),
            DashboardEvent::DiffStats(self.diff_stats.clone()),
            DashboardEvent::Preparing(self.preparing.clone()),
        ]
    }

    fn repos_event(&self) -> DashboardEvent {
        DashboardEvent::Repos {
            repos: self.repos.clone(),
            worktrees_dir: self.worktrees_dir.clone(),
        }
    }

    /// Every open feed gets the event; feeds the app dropped go.
    fn send(&mut self, event: DashboardEvent) {
        self.feeds
            .retain(|feed| feed.unbounded_send(event.clone()).is_ok());
    }

    fn tasks_changed(&mut self) {
        self.send(DashboardEvent::Tasks(self.tasks.clone()));
    }

    fn task_mut(&mut self, id: &str) -> Result<&mut Task, ClientError> {
        self.tasks
            .iter_mut()
            .find(|task| task.id == id)
            .ok_or_else(|| not_found("Task", id))
    }

    fn new_task(&mut self, title: &str, repo: Option<String>) -> Task {
        self.next_number += 1;
        Task {
            id: format!("T-{:04}", self.next_number),
            title: title.into(),
            repo: repo.or_else(|| self.repos.first().map(|repo| repo.id.clone())),
            stage: "spec".into(),
            created_at: NOW.into(),
            updated_at: NOW.into(),
            ..Task::default()
        }
    }
}

fn not_found(what: &str, id: &str) -> ClientError {
    ClientError::Status {
        status: 404,
        message: format!("{what} {id} not found"),
    }
}

/// The state a person's event moves a task to, or None when the fake leaves it.
fn state_after(kind: &str) -> Option<TaskState> {
    Some(match kind {
        "task.readied" | "task.unblocked" => TaskState::Ready,
        "task.blocked" => TaskState::Blocked,
        "agent.resumed" | "task.retried" | "review.changes-requested" => TaskState::Running,
        "review.approved" => TaskState::Done,
        "task.cancelled" => TaskState::Cancelled,
        _ => return None,
    })
}

pub struct FakeDaemon {
    data: Mutex<Data>,
}

impl Default for FakeDaemon {
    fn default() -> Self {
        FakeDaemon {
            data: Mutex::new(Data::seed()),
        }
    }
}

/// The one fake of the app, shared by every `Source::FakeDaemon`.
pub fn shared() -> &'static FakeDaemon {
    static FAKE: OnceLock<FakeDaemon> = OnceLock::new();
    FAKE.get_or_init(FakeDaemon::default)
}

type Answer<T> = Result<T, ClientError>;

impl FakeDaemon {
    fn data(&self) -> MutexGuard<'_, Data> {
        // The data stays whole after a panic in a test: each write is one assignment.
        self.data.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The dashboard feed: everything now, then each change.
    pub fn watch(
        &self,
    ) -> (
        UnboundedReceiver<DashboardEvent>,
        UnboundedSender<DashboardEvent>,
    ) {
        let (sender, receiver) = unbounded();
        let mut data = self.data();
        for event in data.everything() {
            // The receiver is right here: a send cannot fail.
            sender.unbounded_send(event).ok();
        }
        data.feeds.push(sender.clone());
        (receiver, sender)
    }

    /// Changes the data before a window watches it, for the shots of `app/shots.rs`.
    /// `tasks`/`repos` false: the fake has none of them.
    pub fn set_up(&self, link: FakeLink, tasks: bool, repos: bool) {
        let mut data = self.data();
        data.link = link;
        if !tasks {
            data.tasks.clear();
        }
        if !repos {
            data.repos.clear();
        }
    }

    /// The terminal rows of these tasks, to one feed. Tasks with no terminals send nothing, as
    /// the daemon's 404 does.
    pub fn send_terminals(&self, feed: &UnboundedSender<DashboardEvent>, ids: &BTreeSet<String>) {
        let data = self.data();
        for id in ids {
            if let Some(terminals) = data.terminals.get(id) {
                let event = DashboardEvent::Terminals {
                    task_id: id.clone(),
                    terminals: terminals.clone(),
                };
                feed.unbounded_send(event).ok();
            }
        }
    }

    pub fn list_templates(&self) -> Answer<Vec<TaskTemplate>> {
        Ok(self.data().templates.clone())
    }

    pub fn list_events(&self, task_id: &str) -> Answer<Vec<StoredEvent>> {
        Ok(self.data().events.get(task_id).cloned().unwrap_or_default())
    }

    pub fn list_branches(&self, _repo: Option<&str>) -> Answer<BranchList> {
        Ok(self.data().branches.clone())
    }

    pub fn list_skills(&self, _repo_id: &str) -> Answer<SkillList> {
        Ok(self.data().skills.clone())
    }

    pub fn list_changes(&self, task_id: &str, _fresh: bool) -> Answer<Vec<ChangeEntry>> {
        Ok(self
            .data()
            .changes
            .get(task_id)
            .cloned()
            .unwrap_or_default())
    }

    pub fn graph(&self, task_id: &str, _fresh: bool) -> Answer<GitGraph> {
        Ok(self.data().graphs.get(task_id).cloned().unwrap_or_default())
    }

    pub fn list_terminals(&self, task_id: &str) -> Answer<TerminalList> {
        let data = self.data();
        let terminals = data.terminals.get(task_id).cloned();
        terminals.ok_or_else(|| not_found("Terminals of", task_id))
    }

    pub fn create_task(&self, input: NewTaskInput) -> Answer<Task> {
        let mut data = self.data();
        let mut task = data.new_task(&input.title, input.repo);
        task.parent_id = input.parent_id;
        data.tasks.insert(0, task.clone());
        data.tasks_changed();
        Ok(task)
    }

    pub fn create_from_template(
        &self,
        template_id: &str,
        input: TemplateTaskInput,
    ) -> Answer<Task> {
        let mut data = self.data();
        let template = data
            .templates
            .iter()
            .find(|template| template.id == template_id)
            .cloned()
            .ok_or_else(|| not_found("Template", template_id))?;
        let mut task = data.new_task(&input.title, input.repo);
        task.description = template.description;
        task.priority = template.priority;
        task.labels = template.labels;
        data.tasks.insert(0, task.clone());
        data.tasks_changed();
        Ok(task)
    }

    pub fn send_event(&self, task_id: &str, input: TaskEventInput) -> Answer<SendEventAnswer> {
        let mut data = self.data();
        let next = state_after(&input.kind);
        let task = data.task_mut(task_id)?;
        if let Some(state) = next {
            task.state = state;
            task.updated_at = NOW.into();
        }
        let task = task.clone();
        data.tasks_changed();
        Ok(SendEventAnswer {
            task,
            accepted: next.is_some(),
        })
    }

    pub fn rename_task(&self, task_id: &str, title: &str) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        task.title = title.into();
        task.updated_at = NOW.into();
        let task = task.clone();
        data.tasks_changed();
        Ok(task)
    }

    pub fn delete_task(&self, task_id: &str) -> Answer<DeleteTaskAnswer> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        data.tasks.retain(|task| task.id != task_id);
        data.tasks_changed();
        Ok(DeleteTaskAnswer {
            id: task_id.into(),
            tasks: Vec::new(),
        })
    }

    pub fn remove_repo(&self, repo_id: &str) -> Answer<()> {
        let mut data = self.data();
        if !data.repos.iter().any(|repo| repo.id == repo_id) {
            return Err(not_found("Project", repo_id));
        }
        data.repos.retain(|repo| repo.id != repo_id);
        let event = data.repos_event();
        data.send(event);
        Ok(())
    }

    /// The workspace is ready at once; its steps still go out, for the toasts.
    pub fn new_workspace(&self, input: NewWorkspaceInput) -> Answer<Task> {
        let mut data = self.data();
        let mut task = data.new_task(&input.title, input.repo);
        let folder = data.worktrees_dir.clone().unwrap_or_else(|| "/work".into());
        task.workspace = Some(format!("{folder}/{}", task.id));
        task.branch = Some(format!("natamux/{}", task.id.to_lowercase()));
        task.agent = input.agent.or_else(|| Some("claude-code".into()));
        task.state = TaskState::Running;
        task.stage = "implement".into();
        task.context = input.context;
        data.tasks.insert(0, task.clone());
        data.tasks_changed();
        for step in [
            Some(PreparingStep::Fetch),
            Some(PreparingStep::Checkout),
            None,
        ] {
            data.send(DashboardEvent::WorkspaceStep(PreparingMessage {
                task_id: task.id.clone(),
                step,
                error: None,
            }));
        }
        Ok(task)
    }

    pub fn remove_workspace(
        &self,
        task_id: &str,
        delete_branch: bool,
    ) -> Answer<RemoveWorkspaceAnswer> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        task.workspace = None;
        if delete_branch {
            task.branch = None;
        }
        let task = task.clone();
        data.terminals.remove(task_id);
        data.diff_stats.remove(task_id);
        data.tasks_changed();
        let stats = DashboardEvent::DiffStats(data.diff_stats.clone());
        data.send(stats);
        Ok(RemoveWorkspaceAnswer {
            task,
            branch_deleted: delete_branch,
        })
    }

    pub fn start_agent(&self, task_id: &str, input: StartAgentInput) -> Answer<Task> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?;
        task.agent = Some(input.agent);
        task.state = TaskState::Running;
        task.updated_at = NOW.into();
        let task = task.clone();
        data.tasks_changed();
        Ok(task)
    }

    pub fn close_terminal(&self, task_id: &str, terminal: &str) -> Answer<()> {
        let mut data = self.data();
        let Some(list) = data.terminals.get_mut(task_id) else {
            return Ok(());
        };
        list.terminals.retain(|name| name != terminal);
        list.agents.retain(|name| name != terminal);
        list.lanes.remove(terminal);
        list.programs.remove(terminal);
        let event = DashboardEvent::Terminals {
            task_id: task_id.into(),
            terminals: list.clone(),
        };
        data.send(event);
        Ok(())
    }
}

pub(super) fn events<T>(items: impl IntoIterator<Item = T>) -> UnboundedReceiver<T> {
    let (sender, receiver) = unbounded();
    for item in items {
        // The receiver is right here: a send cannot fail.
        sender.unbounded_send(item).ok();
    }
    receiver
}

#[path = "fake_terminals.rs"]
mod screens;

#[path = "fake_stages.rs"]
mod stages;

#[path = "fake_git.rs"]
mod git;

#[path = "fake_settings.rs"]
mod settings;

#[cfg(test)]
#[path = "fake_daemon_tests.rs"]
mod tests;
