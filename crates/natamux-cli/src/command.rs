//! The command line as a command (`packages/cli/src/command.ts`), with the same rules and texts.

use natamux_proto::events::task_event::{EVENT_SOURCES, sendable_event_types};
use natamux_proto::projects::repo::{CONTEXT_SOURCE_TYPES, is_repo_id};
use natamux_proto::tasks::task::{TASK_PRIORITIES, TASK_STATES, is_task_id};
use serde_json::{Map, Value};

use crate::args::{Parsed, parse_args};

/// `back` is the daemon's `reopen`.
pub const STAGE_COMMANDS: [&str; 6] = ["approve", "back", "skip", "pass", "fail", "start"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskChanges {
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub labels: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub source: String,
    pub reference: String,
}

/// `task create`: a struct of its own, as it has many fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTask {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub labels: Vec<String>,
    pub agent: Option<String>,
    pub depends_on: Vec<String>,
    pub context: Option<Context>,
    /// Paths of files for the agents to read.
    pub attach: Option<Vec<String>>,
    /// Without it, the project of NATAMUX_TASK_ID's task, or the only project.
    pub repo: Option<String>,
    /// Makes the task a subtask; the daemon takes the parent's project.
    pub parent_id: Option<String>,
    pub json: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    TaskCreate(NewTask),
    TaskList {
        state: Option<String>,
        json: bool,
    },
    TaskEdit {
        id: String,
        changes: TaskChanges,
        json: bool,
    },
    AgentStart {
        id: String,
        agent: String,
        prompt: Option<String>,
        base: Option<String>,
        branch: Option<String>,
        json: bool,
    },
    /// For an agent without hooks: it waits for the user. Without --task, NATAMUX_TASK_ID.
    AgentAsk {
        id: Option<String>,
        question: String,
        stage: Option<String>,
        json: bool,
    },
    /// The stage this terminal works on is done; the daemon then checks it.
    AgentDone {
        id: Option<String>,
        stage: Option<String>,
        json: bool,
    },
    StageAct {
        id: String,
        action: String,
        stage: Option<String>,
        to: Option<String>,
        reason: Option<String>,
        json: bool,
    },
    /// Whether a stage of the task runs as a workflow the next time it starts.
    StageWorkflow {
        id: String,
        stage: String,
        on: bool,
        json: bool,
    },
    StageCi {
        id: String,
        stage: String,
        on: bool,
        json: bool,
    },
    TaskLink {
        id: String,
        provider: String,
        key: String,
        url: String,
        json: bool,
    },
    /// Without an id, the task of this terminal (NATAMUX_TASK_ID).
    TaskShow {
        id: Option<String>,
        json: bool,
    },
    EventSend {
        id: String,
        kind: String,
        source: String,
        payload: Map<String, Value>,
        json: bool,
    },
    EventList {
        id: String,
        json: bool,
    },
    HooksClaudeCode,
    WorkspaceCreate {
        id: String,
        base: Option<String>,
        branch: Option<String>,
        json: bool,
    },
    WorkspaceRemove {
        id: String,
        delete_branch: bool,
        json: bool,
    },
    RepoTrust(crate::repo_trust::RepoTrustCommand),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageError(pub String);

fn usage<T>(message: impl Into<String>) -> Result<T, UsageError> {
    Err(UsageError(message.into()))
}

fn one_of(value: &str, allowed: &[&str], flag: &str) -> Result<String, UsageError> {
    if allowed.contains(&value) {
        Ok(value.to_string())
    } else {
        usage(format!("{flag} must be one of: {}", allowed.join(", ")))
    }
}

fn task_id(value: Option<&str>) -> Result<String, UsageError> {
    let Some(value) = value else {
        return usage("A task id is required, for example T-42");
    };
    if !is_task_id(value) {
        return usage(format!(
            "\"{value}\" is not a task id. A task id looks like T-42 or ABC-42"
        ));
    }
    Ok(value.to_string())
}

fn task_ids(values: &[String]) -> Result<Vec<String>, UsageError> {
    values.iter().map(|id| task_id(Some(id))).collect()
}

fn repo_id(value: &str) -> Result<String, UsageError> {
    if !is_repo_id(value) {
        return usage(format!(
            "\"{value}\" is not a project id. A project id looks like my-app"
        ));
    }
    Ok(value.to_string())
}

fn payload_object(text: &str) -> Result<Map<String, Value>, UsageError> {
    match serde_json::from_str::<Value>(text) {
        Err(_) => usage("--payload must be valid JSON"),
        Ok(Value::Object(object)) => Ok(object),
        Ok(_) => usage("--payload must be a JSON object, for example '{\"to\":\"ready\"}'"),
    }
}

/// "github-issue:42" or "manual" for --context.
fn context_option(value: &str) -> Result<Context, UsageError> {
    let (source, reference) = match value.split_once(':') {
        Some((source, reference)) => (source, reference.trim()),
        None => (value, ""),
    };
    Ok(Context {
        source: one_of(
            source,
            &CONTEXT_SOURCE_TYPES,
            "The context source in --context",
        )?,
        reference: reference.to_string(),
    })
}

fn no_extra_arguments(extra: &[String]) -> Result<(), UsageError> {
    if extra.is_empty() {
        Ok(())
    } else {
        usage(format!("Unexpected argument: {}", extra.join(" ")))
    }
}

fn owned(value: Option<&str>) -> Option<String> {
    value.map(String::from)
}

/// The parts of a command line that every command group reads.
struct Line<'a> {
    second: Option<&'a str>,
    /// The words after the group and its command.
    rest: &'a [String],
    /// `rest` without its first word, the task id of most commands.
    after_first: &'a [String],
    /// Every word after the group.
    words: &'a [String],
    values: &'a Parsed,
    json: bool,
}

impl Line<'_> {
    fn first(&self) -> Option<&str> {
        self.rest.first().map(String::as_str)
    }

    fn text(&self, name: &str) -> Option<String> {
        owned(self.values.text(name))
    }

    /// `--task`, checked as a task id.
    fn task_option(&self) -> Result<Option<String>, UsageError> {
        self.values
            .text("task")
            .map(|id| task_id(Some(id)))
            .transpose()
    }

    fn priority(&self) -> Result<Option<String>, UsageError> {
        self.values
            .text("priority")
            .map(|priority| one_of(priority, &TASK_PRIORITIES, "--priority"))
            .transpose()
    }
}

pub fn parse_command(argv: &[String]) -> Result<Command, UsageError> {
    let values = parse_args(argv).map_err(UsageError)?;
    let positionals = &values.positionals;
    let Some(group) = positionals.first().map(String::as_str) else {
        return Ok(Command::Help);
    };
    if values.flag("help") || group == "help" {
        return Ok(Command::Help);
    }
    let rest = positionals.get(2..).unwrap_or_default();
    let line = Line {
        second: positionals.get(1).map(String::as_str),
        rest,
        after_first: rest.get(1..).unwrap_or_default(),
        words: positionals.get(1..).unwrap_or_default(),
        values: &values,
        json: values.flag("json"),
    };
    match group {
        "task" => task_command(&line),
        "event" => event_command(&line),
        "agent" => agent_command(&line),
        "ask" => ask_command(&line),
        "done" => {
            no_extra_arguments(line.words)?;
            Ok(Command::AgentDone {
                id: line.task_option()?,
                stage: line.text("stage"),
                json: line.json,
            })
        }
        "stage" => stage_command(&line),
        "events" => {
            no_extra_arguments(line.rest)?;
            Ok(Command::EventList {
                id: task_id(line.second)?,
                json: line.json,
            })
        }
        "hooks" => {
            if line.second != Some("claude-code") {
                return usage(format!(
                    "Unknown hooks command \"{}\". Use claude-code",
                    line.second.unwrap_or_default()
                ));
            }
            no_extra_arguments(line.rest)?;
            Ok(Command::HooksClaudeCode)
        }
        "workspace" => workspace_command(&line),
        "repo" => crate::repo_trust::parse(line.second, line.rest, line.values, line.json)
            .map(Command::RepoTrust)
            .map_err(UsageError),
        _ => usage(format!("Unknown command \"{group}\"")),
    }
}

fn event_command(line: &Line) -> Result<Command, UsageError> {
    let id = task_id(line.second)?;
    let Some(kind) = line.rest.first() else {
        return usage("An event type is required: natamux event T-42 agent.started");
    };
    no_extra_arguments(line.after_first)?;
    let sendable: Vec<&str> = sendable_event_types().collect();
    Ok(Command::EventSend {
        id,
        kind: one_of(kind, &sendable, "The event type")?,
        source: one_of(
            line.values.text("source").unwrap_or("human"),
            &EVENT_SOURCES,
            "--source",
        )?,
        payload: match line.values.text("payload") {
            Some(text) => payload_object(text)?,
            None => Map::new(),
        },
        json: line.json,
    })
}

fn agent_command(line: &Line) -> Result<Command, UsageError> {
    if line.second != Some("start") {
        return usage(format!(
            "Unknown agent command \"{}\". Use start",
            line.second.unwrap_or_default()
        ));
    }
    no_extra_arguments(line.after_first)?;
    Ok(Command::AgentStart {
        id: task_id(line.first())?,
        agent: line
            .values
            .text("agent")
            .unwrap_or("claude-code")
            .to_string(),
        prompt: line.text("prompt"),
        base: line.text("base"),
        branch: line.text("branch"),
        json: line.json,
    })
}

fn ask_command(line: &Line) -> Result<Command, UsageError> {
    let question = line.words.join(" ").trim().to_string();
    if question.is_empty() {
        return usage("A question is required: natamux ask \"Which database should I use?\"");
    }
    Ok(Command::AgentAsk {
        id: line.task_option()?,
        question,
        stage: line.text("stage"),
        json: line.json,
    })
}

fn workspace_command(line: &Line) -> Result<Command, UsageError> {
    if line.second != Some("create") && line.second != Some("remove") {
        return usage(format!(
            "Unknown workspace command \"{}\". Use create or remove",
            line.second.unwrap_or_default()
        ));
    }
    let id = task_id(line.first())?;
    no_extra_arguments(line.after_first)?;
    if line.second == Some("create") {
        return Ok(Command::WorkspaceCreate {
            id,
            base: line.text("base"),
            branch: line.text("branch"),
            json: line.json,
        });
    }
    if !line.values.flag("yes") {
        return usage("Removing a workspace deletes its worktree folder. Add --yes to confirm.");
    }
    Ok(Command::WorkspaceRemove {
        id,
        delete_branch: line.values.flag("delete-branch"),
        json: line.json,
    })
}

fn task_command(line: &Line) -> Result<Command, UsageError> {
    match line.second {
        Some("create") => task_create(line),
        Some("link") => task_link(line),
        Some("list") => {
            no_extra_arguments(line.rest)?;
            Ok(Command::TaskList {
                state: line
                    .values
                    .text("state")
                    .map(|state| one_of(state, &TASK_STATES, "--state"))
                    .transpose()?,
                json: line.json,
            })
        }
        Some("show") => {
            no_extra_arguments(line.after_first)?;
            Ok(Command::TaskShow {
                id: line.first().map(|id| task_id(Some(id))).transpose()?,
                json: line.json,
            })
        }
        Some("edit") => task_edit(line),
        _ => usage(format!(
            "Unknown task command \"{}\". Use create, list, show, edit or link",
            line.second.unwrap_or_default()
        )),
    }
}

fn task_create(line: &Line) -> Result<Command, UsageError> {
    let title = line.rest.join(" ").trim().to_string();
    if title.is_empty() {
        return usage("A title is required: natamux task create \"Fix the bug\"");
    }
    let values = line.values;
    Ok(Command::TaskCreate(NewTask {
        title,
        description: line.text("description"),
        priority: line.priority()?,
        labels: values.texts("label").unwrap_or_default().to_vec(),
        agent: line.text("agent"),
        depends_on: task_ids(values.texts("depends-on").unwrap_or_default())?,
        context: values.text("context").map(context_option).transpose()?,
        repo: values.text("repo").map(repo_id).transpose()?,
        parent_id: values
            .text("parent")
            .map(|id| task_id(Some(id)))
            .transpose()?,
        attach: values.texts("attach").map(<[String]>::to_vec),
        json: line.json,
    }))
}

fn task_link(line: &Line) -> Result<Command, UsageError> {
    no_extra_arguments(line.after_first)?;
    let (Some(key), Some(url)) = (line.values.text("issue"), line.values.text("url")) else {
        return usage(
            "Give the issue and its address: natamux task link T-42 --issue ABC-123 --url https://…",
        );
    };
    let id = task_id(line.first())?;
    let provider = match line.values.text("provider") {
        Some(provider) => provider,
        None if url.contains("github.com") => "github",
        None => "tracker",
    };
    Ok(Command::TaskLink {
        id,
        provider: provider.to_string(),
        key: key.to_string(),
        url: url.to_string(),
        json: line.json,
    })
}

fn task_edit(line: &Line) -> Result<Command, UsageError> {
    no_extra_arguments(line.after_first)?;
    let values = line.values;
    let changes = TaskChanges {
        title: line.text("title"),
        description: line.text("description"),
        priority: line.priority()?,
        labels: values.texts("label").map(<[String]>::to_vec),
        depends_on: values.texts("depends-on").map(task_ids).transpose()?,
    };
    if changes == TaskChanges::default() {
        return usage(
            "Nothing to change. Use --title, --description, --priority, --label or --depends-on",
        );
    }
    Ok(Command::TaskEdit {
        id: task_id(line.first())?,
        changes,
        json: line.json,
    })
}

fn stage_command(line: &Line) -> Result<Command, UsageError> {
    let Some(second) = line.second else {
        return usage(format!(
            "A stage command is required: {}",
            STAGE_COMMANDS.join(", ")
        ));
    };
    if second == "workflow" || second == "ci" {
        let id = line.first();
        let choice = line.rest.get(1).map(String::as_str);
        no_extra_arguments(line.rest.get(2..).unwrap_or_default())?;
        let Some(stage) = line.values.text("stage") else {
            return usage("Name the stage with --stage, for example --stage implement");
        };
        if choice != Some("on") && choice != Some("off") {
            return usage(format!(
                "Say on or off: natamux stage {second} T-42 on --stage implement"
            ));
        }
        let (id, stage, on, json) = (
            task_id(id)?,
            stage.to_string(),
            choice == Some("on"),
            line.json,
        );
        return Ok(if second == "ci" {
            Command::StageCi {
                id,
                stage,
                on,
                json,
            }
        } else {
            Command::StageWorkflow {
                id,
                stage,
                on,
                json,
            }
        });
    }
    let action = one_of(second, &STAGE_COMMANDS, "The stage command")?;
    no_extra_arguments(line.after_first)?;
    if action == "back" && line.values.text("stage").is_some() {
        return usage("stage back goes to the stage named with --to, not --stage");
    }
    Ok(Command::StageAct {
        id: task_id(line.first())?,
        action,
        stage: line.text("stage"),
        to: line.text("to"),
        reason: line.text("reason"),
        json: line.json,
    })
}
