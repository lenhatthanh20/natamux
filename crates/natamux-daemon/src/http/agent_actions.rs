//! What each route does when the agent token of a task calls it (F6). A route that is not
//! classified here answers 403 to agents: a new route stays closed until it is added.

use hyper::Method;

/// What a call does, named as the Strict grants name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentAction {
    TaskReadOwn,
    TaskUpdateOwn,
    EventPostOwn,
    StageReportOwn,
    HookPost,
    LimitsPost,
    SubtaskCreate,
    SubtaskReadStatus,
    TerminalTicketOwn,
    FilesRwOwn,
    WorkspaceCommitOwn,
    AgentMessageOwn,
    GitPushOwn,
    PrCreateOwn,
    IssueCreateOwn,
    AgentStartOwn,
    AgentReopenOwn,
    WorkspaceCreateOwn,
    RollbackOwn,
    SubtaskReadWorkspace,
    EventsSubscribe,
    RepoReadOwn,
    TasksList,
    /// A dashboard read cut to the agent's own rows. No Strict grant gives it.
    GlobalRead,
}

impl AgentAction {
    pub fn name(self) -> &'static str {
        use AgentAction::*;
        match self {
            TaskReadOwn => "task.read.own",
            TaskUpdateOwn => "task.update.own",
            EventPostOwn => "event.post.own",
            StageReportOwn => "stage.report.own",
            HookPost => "hook.post",
            LimitsPost => "limits.post",
            SubtaskCreate => "subtask.create",
            SubtaskReadStatus => "subtask.read.status",
            TerminalTicketOwn => "terminal.ticket.own",
            FilesRwOwn => "files.rw.own",
            WorkspaceCommitOwn => "workspace.commit.own",
            AgentMessageOwn => "agent.message.own",
            GitPushOwn => "git.push.own",
            PrCreateOwn => "pr.create.own",
            IssueCreateOwn => "issue.create.own",
            AgentStartOwn => "agent.start.own",
            AgentReopenOwn => "agent.reopen.own",
            WorkspaceCreateOwn => "workspace.create.own",
            RollbackOwn => "rollback.own",
            SubtaskReadWorkspace => "subtask.read.workspace",
            EventsSubscribe => "events.subscribe",
            RepoReadOwn => "repo.read.own",
            TasksList => "tasks.list",
            GlobalRead => "global.read",
        }
    }

    /// The same read made on a direct subtask instead of the own task.
    pub fn on_child(self) -> AgentAction {
        match self {
            AgentAction::FilesRwOwn => AgentAction::SubtaskReadWorkspace,
            _ => AgentAction::SubtaskReadStatus,
        }
    }
}

/// Which task a route may name for the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// `/tasks/<own>/…` only.
    Own,
    /// `/tasks/<own>/…`, or a read of a direct subtask.
    Child,
    /// No task in the address; the handler cuts the answer to the agent's rows.
    Global,
}

/// The routes no agent token may call, as `METHOD path` with `*` for one segment. A test checks
/// that every route of `server.rs` is either classified or listed here.
pub const AGENT_DENIED: &[(&str, &str)] = &[
    ("GET", "health"),
    ("GET", "custom-agents"),
    ("PUT", "custom-agents"),
    // Turning Strict off or loosening a key needs the person at the app.
    ("PUT", "security"),
    // The bundle shows the security state an agent could plan around.
    ("GET", "support-bundle"),
    // Saved templates and plan limits have no task to cut them to.
    ("GET", "templates"),
    ("GET", "limits"),
    ("POST", "templates"),
    ("POST", "templates/*/tasks"),
    ("DELETE", "tasks/*"),
    ("DELETE", "tasks/*/terminals/*"),
    ("DELETE", "tasks/*/review-comments/*"),
    ("DELETE", "tasks/*/workspace"),
    // Fan-out makes top-level tasks and starts their agents.
    ("POST", "tasks/*/fan-out"),
    // A CI push needs a person's approval (F4).
    ("POST", "tasks/*/stages/*/ci-approve"),
    ("POST", "workspaces"),
    ("POST", "repos"),
    ("DELETE", "repos/*"),
    ("PUT", "repos/*/settings"),
    ("GET", "repos/*/trust"),
    ("POST", "repos/*/trust"),
    ("GET", "github/issues"),
    ("POST", "github/issues/import"),
];

pub fn is_denied(method: &Method, parts: &[&str]) -> bool {
    AGENT_DENIED.iter().any(|(denied, pattern)| {
        let wanted: Vec<&str> = pattern.split('/').collect();
        *denied == method.as_str()
            && wanted.len() == parts.len()
            && wanted
                .iter()
                .zip(parts)
                .all(|(want, part)| *want == "*" || want == part)
    })
}

/// The action and scope of a route; None: agents may not call it.
pub fn classify(method: &Method, parts: &[&str]) -> Option<(AgentAction, Scope)> {
    use AgentAction::*;
    let get = *method == Method::GET;
    let post = *method == Method::POST;
    match parts {
        ["tasks", _, rest @ ..] => task_route(method.as_str(), rest),
        ["tasks"] if get => Some((TasksList, Scope::Global)),
        // `create_task` checks that it is a subtask of the own task.
        ["tasks"] if post => Some((SubtaskCreate, Scope::Global)),
        ["events"] if get => Some((EventsSubscribe, Scope::Global)),
        // No secrets: an agent may see which rules hold it.
        ["security"] if get => Some((TaskReadOwn, Scope::Global)),
        ["hooks", _] if post => Some((HookPost, Scope::Global)),
        ["limits", _] if post => Some((LimitsPost, Scope::Global)),
        ["jobs"] | ["activity"] | ["usage"] | ["diffstats"] | ["workspaces", "preparing"]
            if get =>
        {
            Some((GlobalRead, Scope::Global))
        }
        ["repos"]
        | ["repos", _, "settings" | "skills"]
        | ["git", "branches" | "remote-branches"]
            if get =>
        {
            Some((RepoReadOwn, Scope::Global))
        }
        _ => None,
    }
}

/// The routes under `/tasks/<id>`; `rest` is what follows the id.
fn task_route(method: &str, rest: &[&str]) -> Option<(AgentAction, Scope)> {
    use AgentAction::*;
    let classified = match (method, rest) {
        ("GET", [] | ["events"] | ["review-comments"]) => (TaskReadOwn, Scope::Child),
        ("GET", ["terminals"] | ["ended-shells"]) => (TaskReadOwn, Scope::Own),
        ("GET", ["terminal"]) | ("POST", ["terminal", "ticket"]) => (TerminalTicketOwn, Scope::Own),
        (
            "GET",
            ["attachments"]
            | ["attachments", _]
            | ["files"]
            | ["files", _]
            | ["changes"]
            | ["changes", "file" | "image"]
            | ["blame"]
            | ["tree"]
            | ["file"]
            | ["graph"],
        ) => (FilesRwOwn, Scope::Child),
        ("PUT", ["files", _] | ["file"]) => (FilesRwOwn, Scope::Own),
        ("PATCH", [] | ["review-comments", _])
        | ("PUT", ["issue" | "workflow" | "ci" | "skip"])
        | ("POST", ["review-comments"] | ["review-comments", "send"]) => {
            (TaskUpdateOwn, Scope::Own)
        }
        ("POST", ["events"]) => (EventPostOwn, Scope::Own),
        ("POST", ["stage"]) => (StageReportOwn, Scope::Own),
        ("POST", ["launch-check"] | ["pull-request", "refresh"]) => (TaskReadOwn, Scope::Own),
        ("POST", ["agent"]) => (AgentStartOwn, Scope::Own),
        ("POST", ["agent", "message"]) => (AgentMessageOwn, Scope::Own),
        ("POST", ["agent", "reopen"]) => (AgentReopenOwn, Scope::Own),
        ("POST", ["workspace"]) => (WorkspaceCreateOwn, Scope::Own),
        ("POST", ["commit"] | ["commit-message"]) => (WorkspaceCommitOwn, Scope::Own),
        ("POST", ["rollback"] | ["rollback", "undo"]) => (RollbackOwn, Scope::Own),
        ("POST", ["push"]) => (GitPushOwn, Scope::Own),
        ("POST", ["pull-request"]) => (PrCreateOwn, Scope::Own),
        ("POST", ["issue"]) => (IssueCreateOwn, Scope::Own),
        _ => return None,
    };
    Some(classified)
}

#[cfg(test)]
#[path = "agent_actions_tests.rs"]
mod tests;
