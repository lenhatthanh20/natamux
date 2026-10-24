//! Ports `packages/daemon/src/stages/stage-prompts.ts`.

use natamux_agents::skill_resolve::SkillResolution;
use natamux_core::js::js_trim;
use natamux_core::tasks::attachments::format_bytes;
use natamux_proto::projects::repo::ContextSource;
use natamux_proto::stages::prompt::{commit_rule, default_prompt};
use natamux_proto::stages::schema::Stage;
use natamux_proto::tasks::attachments::Attachment;
use natamux_proto::tasks::task::Task;
use natamux_store::attachments::attachment_path;
use natamux_store::task_files::STAGE_DIR;

/// A prompt of your own may say when not to run it, for example after a send back.
pub const DONE: &str = "When you are done, run `natamux done`, unless the steps above say not to.";

/// Claude Code runs a workflow only when the prompt asks for one in words; "ultracode" is its
/// keyword for that.
pub const WORKFLOW_LINE: &str = "ultracode: run this stage as a workflow. Split the work across sub-agents, and check the result before you finish.";

/// Replaces `{word}` (ASCII `\w+`) with its value; a word with no value stays as it is.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let word = after
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if word > 0 && after.as_bytes().get(word) == Some(&b'}') {
            let key = &after[..word];
            match values.iter().find(|(name, _)| *name == key) {
                Some((_, value)) => out.push_str(value),
                None => out.push_str(&rest[open..open + word + 2]),
            }
            rest = &after[word + 1..];
        } else {
            out.push('{');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn reference(task: &Task) -> &str {
    task.context
        .as_ref()
        .map_or("", |context| context.reference.as_str())
}

fn values(task: &Task) -> [(&'static str, &str); 4] {
    [
        ("title", &task.title),
        ("description", &task.description),
        ("dir", STAGE_DIR),
        ("ref", reference(task)),
    ]
}

/// The prompt an agent gets when its stage starts. `{skill:<name>}` is left for `fill_skills`,
/// which knows the agent. The workflow line goes outside the template, so a prompt of your own
/// gets it too.
pub fn stage_prompt(stage: &Stage, task: &Task, workflow: bool) -> String {
    let template = stage
        .prompt
        .clone()
        .unwrap_or_else(|| default_prompt(stage));
    let mut all = values(task).to_vec();
    // Only the default prompts get {commit}, so a prompt of your own is never changed.
    if stage.prompt.is_none() {
        all.push(("commit", commit_rule(stage)));
    }
    let mut lines = vec![fill(&template, &all)];
    if workflow {
        lines.push(WORKFLOW_LINE.to_string());
    }
    lines.push(DONE.to_string());
    lines.join("\n")
}

/// The prompt for a context source that an agent reads: a skill or an MCP server. `skill` says
/// how the task's agent gets the skill (see `resolve_skill`).
pub fn context_prompt(
    source: &ContextSource,
    task: &Task,
    attachments: &[Attachment],
    skill: Option<&SkillResolution>,
) -> String {
    let ask = if source.kind == "skill" {
        let name = source.skill.as_deref().unwrap_or_default();
        format!(
            "{}\nWrite what you found to {{dir}}/context.md.",
            use_skill(name, skill)
        )
    } else {
        source.prompt.clone().unwrap_or_default()
    };
    let mut lines = vec![fill(&ask, &values(task))];
    lines.extend(attachment_lines(attachments));
    lines.push(DONE.to_string());
    lines.join("\n")
}

/// Without a resolution the skill is called the way Claude Code calls it.
pub fn use_skill(name: &str, skill: Option<&SkillResolution>) -> String {
    match skill {
        None => format!("/{name} {{ref}}"),
        Some(SkillResolution::Call(text)) => format!("{text} {{ref}}"),
        Some(SkillResolution::File(path)) => format!("Read {path} and follow it for {{ref}}."),
        Some(SkillResolution::Missing) => format!("Use the {name} skill for {{ref}}."),
    }
}

/// The later stages find the attachments only through the context, so its agent must list them.
pub fn attachment_lines(attachments: &[Attachment]) -> Vec<String> {
    if attachments.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!(
        "Files are attached to the task. Read them too, and list them under an Attachments heading in {STAGE_DIR}/context.md:"
    )];
    lines.extend(attachments.iter().map(|attachment| {
        format!(
            "- {} ({})",
            attachment_path(&attachment.name),
            format_bytes(attachment.size)
        )
    }));
    lines
}

fn notes_of(task: &Task) -> String {
    let notes = js_trim(&task.description);
    if notes.is_empty() {
        "(No notes.)".into()
    } else {
        notes.into()
    }
}

/// A subtask's context ends with its parent: the agent reads the parent's spec and plan there.
pub fn with_parent_section(text: &str, parent: &Task, folder: &str) -> String {
    let gap = if text.is_empty() || text.ends_with("\n\n") {
        ""
    } else if text.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    [
        format!("{text}{gap}## Parent task"),
        String::new(),
        format!("{}: {}", parent.id, parent.title),
        String::new(),
        notes_of(parent),
        String::new(),
        format!("Stage files of the parent: {folder}"),
        String::new(),
    ]
    .join("\n")
}

/// The context of a task whose source is typed by hand: its title, notes and reference.
pub fn manual_context(task: &Task) -> String {
    let reference = reference(task);
    let mut lines = vec![format!("# {}", task.title), String::new(), notes_of(task)];
    if !reference.is_empty() {
        lines.push(String::new());
        lines.push(format!("Reference: {reference}"));
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
