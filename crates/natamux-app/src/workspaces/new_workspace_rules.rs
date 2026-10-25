//! Pure rules of the Start a task dialog (`NewWorkspaceDialog.tsx`, `AttachmentField.tsx`,
//! `lib/attachments.ts`): the context sources it offers and their hints, the project and branch
//! it starts from, the request it sends, and the files it takes.

use std::path::PathBuf;

use natamux_proto::projects::repo::{BranchList, ContextSource};
use natamux_proto::tasks::attachments::{AttachmentUpload, attachment_limit_error};
use natamux_proto::tasks::task::TaskContext;
use natamux_proto::workspaces::workspace::NewWorkspaceInput;

use super::branches::branch_options;
use super::workspace_rules::create_blocked_reason;
use crate::sidebar::workspace_groups::DEFAULT_REPO_ID;

/// Where the task's context comes from: a source of the project settings, or any skill the
/// agents have, picked in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceChoice {
    Project(ContextSource),
    AgentSkill,
}

pub fn manual() -> SourceChoice {
    SourceChoice::Project(ContextSource {
        kind: "manual".into(),
        name: None,
        skill: None,
        agent: None,
        prompt: None,
    })
}

impl SourceChoice {
    /// One of `CONTEXT_SOURCE_TYPES`.
    pub fn kind(&self) -> &str {
        match self {
            SourceChoice::Project(source) => &source.kind,
            SourceChoice::AgentSkill => "skill",
        }
    }

    fn name(&self) -> &str {
        match self {
            SourceChoice::Project(source) => source.name.as_deref().unwrap_or(""),
            SourceChoice::AgentSkill => "",
        }
    }

    /// Two skills or two MCP servers share a type, so their names tell the options apart. The
    /// agent skill's empty name keeps it apart from the skills of the settings.
    pub fn key(&self) -> String {
        match self {
            SourceChoice::AgentSkill => "skill:".into(),
            SourceChoice::Project(source) => match &source.name {
                Some(name) => format!("{}:{name}", source.kind),
                None => source.kind.clone(),
            },
        }
    }

    pub fn label(&self) -> String {
        if *self == SourceChoice::AgentSkill {
            return "An agent skill".into();
        }
        match self.kind() {
            "manual" => "My notes".into(),
            "github-issue" => "A GitHub issue".into(),
            "skill" => format!("{} (skill)", self.name()),
            "mcp" => format!("{} (MCP server)", self.name()),
            other => other.into(),
        }
    }

    /// The skill a source runs. An agent skill runs the one picked, when there is one.
    fn skill_of(&self, skill: &str) -> String {
        match self {
            SourceChoice::Project(_) => self.name().into(),
            SourceChoice::AgentSkill if skill.is_empty() => "picked".into(),
            SourceChoice::AgentSkill => skill.into(),
        }
    }

    /// The line under the Context heading: what becomes the task's context (context.md).
    pub fn context_hint(&self, skill: &str) -> String {
        let reads = match self.kind() {
            "github-issue" => {
                "reads the issue (its title, text, labels and comments) and the files".into()
            }
            "skill" => format!(
                "runs the {} skill on the reference, and reads what it finds and the files",
                self.skill_of(skill)
            ),
            "mcp" => format!(
                "asks the {} MCP server about the reference, and reads what it finds and the files",
                self.name()
            ),
            _ => "reads the title, the notes and the files".to_string(),
        };
        format!("The agent {reads} as the task's context.")
    }

    /// The line under the Reference field. The placeholder goes away once the user types, so this
    /// line keeps saying it.
    pub fn ref_hint(&self, skill: &str) -> String {
        match self.kind() {
            "github-issue" => {
                "The issue number, such as #42, or its address. Natamux reads the issue from GitHub."
                    .into()
            }
            "skill" if *self == SourceChoice::AgentSkill => format!(
                "The key or address of the record. The {} skill reads it.",
                self.skill_of(skill)
            ),
            "skill" => format!(
                "The key or address of the ticket. The {} skill reads it.",
                self.name()
            ),
            "mcp" => format!(
                "The key or address of the ticket. The agent reads it through the {} MCP server.",
                self.name()
            ),
            _ => String::new(),
        }
    }

    pub fn ref_placeholder(&self) -> &'static str {
        match self.kind() {
            "github-issue" => "#42 or the issue address",
            "skill" => "The ticket key or address",
            "mcp" => "The ticket key or address, such as ABC-123",
            _ => "",
        }
    }

    /// The daemon runs the source with this name, not the first one of its type.
    pub fn context(&self, reference: &str, skill: &str) -> TaskContext {
        let reference = reference.trim().to_string();
        match self {
            SourceChoice::AgentSkill => TaskContext {
                source: "skill".into(),
                name: None,
                skill: Some(skill.into()),
                reference,
            },
            SourceChoice::Project(source) => TaskContext {
                source: source.kind.clone(),
                name: source.name.clone(),
                skill: None,
                reference,
            },
        }
    }
}

/// The sources of the "Context from" select: the project's (My notes alone until its settings
/// load), then An agent skill when the agents have a skill.
pub fn source_choices(configured: Option<&[ContextSource]>, has_skills: bool) -> Vec<SourceChoice> {
    let mut choices: Vec<SourceChoice> = match configured {
        Some(sources) => sources.iter().cloned().map(SourceChoice::Project).collect(),
        None => vec![manual()],
    };
    if has_skills {
        choices.push(SourceChoice::AgentSkill);
    }
    choices
}

/// Another project may not offer the source picked in the first one: then My notes.
pub fn pick_source(choices: &[SourceChoice], key: &str) -> SourceChoice {
    choices
        .iter()
        .find(|choice| choice.key() == key)
        .cloned()
        .unwrap_or_else(manual)
}

/// The project the dialog starts in: the one it was opened for, else the first one.
pub fn initial_repo(opened_for: Option<&str>, repo_ids: &[String]) -> String {
    opened_for
        .map(str::to_string)
        .or_else(|| repo_ids.first().cloned())
        .unwrap_or_else(|| DEFAULT_REPO_ID.into())
}

/// The picked project may be gone, or the list was not loaded when the dialog opened. Then the
/// first project stands in.
pub fn shown_repo(picked: &str, repo_ids: &[String]) -> String {
    if repo_ids.iter().any(|id| id == picked) {
        return picked.into();
    }
    repo_ids.first().cloned().unwrap_or_else(|| picked.into())
}

/// Where the workspace starts when no base is sent: the default branch, else the checkout's.
pub fn start_branch(list: Option<&BranchList>) -> Option<String> {
    let list = list?;
    list.default_branch.clone().or_else(|| list.current.clone())
}

/// The base kept after a pick: empty for the start branch, so none is sent.
pub fn base_after_pick(name: &str, start: Option<&str>) -> String {
    if Some(name) == start {
        String::new()
    } else {
        name.into()
    }
}

/// The text of the "Branch from" button: the picked branch, or the start one, by its option
/// label, such as "main (default)".
pub fn branch_text(list: Option<&BranchList>, base: &str, last_used: Option<&str>) -> String {
    let start = start_branch(list);
    let value = if base.is_empty() {
        start.clone()
    } else {
        Some(base.to_string())
    };
    let Some(value) = value else {
        return "The default branch".into();
    };
    let Some(list) = list else {
        return value;
    };
    branch_options(
        &list.branches,
        list.current.as_deref(),
        last_used,
        start.as_deref(),
    )
    .into_iter()
    .find(|option| option.value == value)
    .map_or(value, |option| option.label)
}

/// What the user typed and picked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StartForm {
    pub title: String,
    pub repo: String,
    /// Empty: the start branch.
    pub base: String,
    /// Empty: no agent, a plain terminal.
    pub agent: String,
    pub notes: String,
    pub reference: String,
    /// The skill picked for An agent skill; empty until one is picked.
    pub skill: String,
}

/// Why Start task is off, if it is.
pub fn blocked_reason(form: &StartForm, choice: &SourceChoice) -> Option<&'static str> {
    let skill = (*choice == SourceChoice::AgentSkill).then_some(form.skill.as_str());
    create_blocked_reason(
        &form.title,
        choice.kind(),
        &form.reference,
        skill,
        Some(&form.agent),
    )
}

fn filled(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

/// The body of `POST /workspaces`. Only notes typed by hand are the task's context; another
/// source brings its own.
pub fn new_workspace_input(
    form: &StartForm,
    choice: &SourceChoice,
    attachments: Vec<AttachmentUpload>,
) -> NewWorkspaceInput {
    let manual = choice.kind() == "manual";
    NewWorkspaceInput {
        title: form.title.trim().into(),
        repo: Some(form.repo.clone()),
        base: filled(&form.base),
        agent: filled(&form.agent),
        prompt: if manual {
            filled(form.notes.trim())
        } else {
            None
        },
        context: (!manual).then(|| choice.context(&form.reference, &form.skill)),
        attachments,
    }
}

pub fn start_label(busy: bool) -> &'static str {
    if busy {
        "Starting the task"
    } else {
        "Start task"
    }
}

/// A file picked for the new task. The id keys the row, as two files may share a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedFile {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub path: PathBuf,
}

/// `fitAttachments`: splits added files into those that fit next to the ones already picked and
/// the reasons for those over a limit.
pub fn fit_attachments(
    current: &[PickedFile],
    added: Vec<PickedFile>,
) -> (Vec<PickedFile>, Vec<String>) {
    let mut sized: Vec<(String, u64)> = current
        .iter()
        .map(|file| (file.name.clone(), file.size))
        .collect();
    let mut kept = Vec::new();
    let mut refused = Vec::new();
    for file in added {
        sized.push((file.name.clone(), file.size));
        // The files before fit, so an error can only name this one.
        match attachment_limit_error(&sized) {
            Some(reason) => {
                sized.pop();
                refused.push(reason);
            }
            None => kept.push(file),
        }
    }
    (kept, refused)
}

/// `formatBytes` of core.
pub fn format_bytes(bytes: u64) -> String {
    const MB: u64 = 1_000_000;
    if bytes < 1000 {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / 1000.0)
    } else {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// shortcut: a hand-written encoder, as the app does not depend on the base64 crate; use the
// crate if the app ever needs decoding too.
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                text.push(char::from(BASE64[((n >> shift) & 63) as usize]));
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// `readAsUpload` for each file: the daemon takes them as base64 inside the JSON body. Runs off
/// the main thread.
pub fn read_uploads(files: &[PickedFile]) -> Result<Vec<AttachmentUpload>, String> {
    files
        .iter()
        .map(|file| {
            let bytes =
                std::fs::read(&file.path).map_err(|_| format!("Could not read {}", file.name))?;
            Ok(AttachmentUpload {
                name: file.name.clone(),
                data: base64_encode(&bytes),
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "new_workspace_rules_tests.rs"]
mod tests;
