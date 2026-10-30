//! The text the command prints (`packages/cli/src/format.ts`), letter for letter.

use natamux_proto::events::task_event::{EVENT_SOURCES, StoredEvent, sendable_event_types};
use natamux_proto::tasks::task::{TASK_PRIORITIES, TASK_STATES, Task, TaskContext};
use serde_json::Value;

use crate::daemon::{RemoveWorkspaceResult, SendEventResult};

pub const DEFAULT_URL: &str = natamux_client::config::DEFAULT_URL;

pub fn usage() -> String {
    format!(
        r#"Usage: natamux <command> [options]
       nmx <command> [options]   (short name)

Commands:
  task create <title>        Create a draft task
  task list                  List tasks, newest update first
  task show <id>             Show one task
  task edit <id>             Change the title, notes, priority, labels or dependencies
  task link <id>             Link the task to its issue or ticket (--issue, --url)
  agent start <id>           Start an agent in the task's terminal (default: claude-code)
  ask <question>             For an agent: say the task waits for the user
  done                       For an agent: say its stage is done, so the daemon checks it
  stage <command> <id>       approve, back, skip, pass, fail or start the task's stage
  stage workflow <id> on|off Run a stage as a workflow (Claude Code; needs --stage)
  stage ci <id> on|off       Check a stage on CI before it passes (needs --stage)
  event <id> <type>          Send an event to a task
  events <id>                Show the event log of a task
  workspace create <id>      Create a git worktree and a branch for a task
  workspace remove <id>      Remove the worktree of a task (needs --yes)
  repo trust <repo>          Show the commands a project runs; approve them with --approve
  hooks claude-code          Print Claude Code settings that report a task's progress
  hook <agent>               For an agent's hook: send the JSON on stdin to the daemon (silent)
  help                       Show this help

Options:
  --title <text>        task edit: new title
  --description <text>  task create, task edit: notes
  --priority <name>     task create, task edit: {priorities}
  --label <name>        task create, task edit: label (repeat for more)
  --agent <name>        task create: agent that works on the task; agent start: which agent
  --depends-on <id>     task create, task edit: task that must finish first (repeat for more)
  --prompt <text>       agent start: first prompt (default: the task title and notes)
  --task <id>           ask, done: the task (default: NATAMUX_TASK_ID)
  --context <source>    task create: where its context comes from, such as github-issue:42
  --repo <id>           task create: the project (default: the project of NATAMUX_TASK_ID's
                        task, or the only project)
  --parent <id>         task create: the parent task; makes a subtask
  --attach <path>       task create: a file for the agents to read (repeat for more)
  --stage <id>          stage, done, ask: the stage, when two run side by side; stage workflow,
                        stage ci: the stage to change
  --to <id>             stage back: the stage to go back to (default: the last agent stage)
  --reason <text>       stage back, stage fail: why, for the agent and the log
  --issue <key>         task link: the issue or ticket, such as #42 or ABC-123
  --url <address>       task link: its address
  --provider <name>     task link: github, jira, … (default: from the address)
  --state <state>       task list: only tasks in this state ({states})
  --source <name>       event: {sources} (default: human)
  --payload <json>      event: JSON object sent with the event
  --base <branch>       workspace create, agent start: the new workspace starts from this
                        local branch, or origin/<branch> fetched first (default: main, master
                        or the default of origin)
  --branch <name>       workspace create, agent start: the new workspace uses this branch:
                        local, or origin/<name> tracked, or new from the base
  --yes                 workspace remove: confirm that the worktree folder is deleted
  --delete-branch       workspace remove: also delete the branch when it is merged
  --approve <hash>      repo trust: approve the commands with this hash (from the list)
  --json                Print JSON instead of text
  -h, --help            Show this help

Event types: {events}

Environment:
  NATAMUX_TOKEN         Token of the daemon (required, except for hooks claude-code)
  NATAMUX_URL           Address of the daemon (default: {DEFAULT_URL})
  NATAMUX_TASK_ID       The task of this terminal: its hooks, ask and done report on it.
                        Terminals that Natamux opens set it
  NATAMUX_TERMINAL      The terminal of the task, which tells the stage done is about
"#,
        priorities = TASK_PRIORITIES.join(", "),
        states = TASK_STATES.join(", "),
        sources = EVENT_SOURCES.join(", "),
        events = sendable_event_types().collect::<Vec<_>>().join(", "),
    )
}

/// "jira-ticket (skill) ABC-1": a source with a skill or a name says which one it runs.
fn context_text(context: &TaskContext) -> String {
    let source = match context.skill.as_ref().or(context.name.as_ref()) {
        Some(name) => format!("{name} ({})", context.source),
        None => context.source.clone(),
    };
    format!("{source} {}", context.reference).trim().to_string()
}

/// The length JavaScript gives a string: UTF-16 code units.
fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = header
        .iter()
        .enumerate()
        .map(|(column, title)| {
            rows.iter()
                .map(|row| row.get(column).map_or(0, |cell| js_len(cell)))
                .max()
                .unwrap_or(0)
                .max(js_len(title))
        })
        .collect();
    let line = |cells: &[&str]| {
        cells
            .iter()
            .enumerate()
            .map(|(column, cell)| {
                let width = widths.get(column).copied().unwrap_or(0);
                format!("{cell}{}", " ".repeat(width.saturating_sub(js_len(cell))))
            })
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut lines = vec![line(header)];
    for row in rows {
        lines.push(line(&row.iter().map(String::as_str).collect::<Vec<_>>()));
    }
    lines.join("\n")
}

pub fn format_task_list(tasks: &[Task]) -> String {
    if tasks.is_empty() {
        return "No tasks.".into();
    }
    let rows: Vec<Vec<String>> = tasks
        .iter()
        .map(|task| {
            vec![
                task.id.clone(),
                task.state.name().to_string(),
                task.priority.clone(),
                task.title.clone(),
            ]
        })
        .collect();
    table(&["ID", "STATE", "PRIORITY", "TITLE"], &rows)
}

fn or_dash(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}

fn joined_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".into()
    } else {
        values.join(", ")
    }
}

fn lane_state(value: &Value) -> String {
    match value {
        Value::String(state) => state.clone(),
        other => other.to_string(),
    }
}

pub fn format_task(task: &Task) -> String {
    let lanes = if task.lanes.is_empty() {
        String::new()
    } else {
        let lanes: Vec<String> = task
            .lanes
            .iter()
            .map(|(stage, state)| format!("{stage} {}", lane_state(state)))
            .collect();
        format!(" ({})", lanes.join(", "))
    };
    let mut lines = vec![
        format!("{}  {}", task.id, task.title),
        format!("Stage:      {}{lanes}", task.stage),
        format!("State:      {}", task.state.name()),
        format!("Priority:   {}", task.priority),
        format!("Labels:     {}", joined_or_dash(&task.labels)),
        format!("Agent:      {}", or_dash(&task.agent)),
        format!("Branch:     {}", or_dash(&task.branch)),
        format!("Workspace:  {}", or_dash(&task.workspace)),
        format!("PR:         {}", or_dash(&task.pr)),
        format!(
            "Issue:      {}",
            task.issue
                .as_ref()
                .map_or("-".into(), |issue| format!("{} {}", issue.key, issue.url))
        ),
        format!(
            "Context:    {}",
            task.context.as_ref().map_or("-".into(), context_text)
        ),
        format!("Repo:       {}", task.repo.as_deref().unwrap_or("default")),
        format!("Depends on: {}", joined_or_dash(&task.depends_on)),
        format!("Created:    {}", task.created_at),
        format!("Updated:    {}", task.updated_at),
    ];
    if !task.description.is_empty() {
        lines.push(String::new());
        lines.push(task.description.clone());
    }
    lines.join("\n")
}

pub fn format_event_result(result: &SendEventResult) -> String {
    if result.accepted {
        format!(
            "Accepted. {} is now {}.",
            result.task.id,
            result.task.state.name()
        )
    } else {
        format!(
            "Not accepted. {} stays in {}.",
            result.task.id,
            result.task.state.name()
        )
    }
}

pub fn format_workspace_created(task: &Task) -> String {
    [
        format!("Created the workspace of {}", task.id),
        format!("Branch:    {}", or_dash(&task.branch)),
        format!("Workspace: {}", or_dash(&task.workspace)),
    ]
    .join("\n")
}

pub fn format_workspace_removed(result: &RemoveWorkspaceResult) -> String {
    let branch = if result.branch_deleted {
        "The branch was deleted.".to_string()
    } else {
        match &result.task.branch {
            None => "The task has no branch.".to_string(),
            Some(branch) => format!("The branch {branch} is kept."),
        }
    };
    format!("Removed the workspace of {}. {branch}", result.task.id)
}

pub fn format_event_log(events: &[StoredEvent]) -> String {
    if events.is_empty() {
        return "No events.".into();
    }
    let rows: Vec<Vec<String>> = events
        .iter()
        .map(|stored| {
            vec![
                stored.seq.to_string(),
                stored.event.at.clone(),
                stored.event.source.clone(),
                stored.event.kind.clone(),
                if stored.accepted { "yes" } else { "no" }.to_string(),
            ]
        })
        .collect();
    table(&["SEQ", "AT", "SOURCE", "TYPE", "ACCEPTED"], &rows)
}
