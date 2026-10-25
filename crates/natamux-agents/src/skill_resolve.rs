//! `resolveSkill` and `fillSkills` of `packages/daemon/src/agents/agent-skills.ts`: how the task's
//! agent gets a skill a prompt names.

use std::path::{Path, PathBuf};

use crate::paths::relative;
use crate::skills::{SKILL_SOURCES, agent_skills, skill_source};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillContext {
    pub agent: String,
    /// The task's worktree: its skills are the ones of the branch the agent works on.
    pub workspace: PathBuf,
    /// The main checkout. Its untracked files are not in a new worktree.
    pub repo: PathBuf,
    pub home: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkillResolution {
    /// The agent's own call, such as `/name`.
    Call(String),
    /// The SKILL.md to read, shown from the workspace when inside it.
    File(String),
    Missing,
}

/// How the task's agent gets a skill: its own call when it has the skill and a way to call it,
/// else the SKILL.md to read (of its own, or of another agent, in the worktree and then in the
/// main checkout), else nothing. A skill of the main checkout only is never called: the agent in
/// the worktree may not have it.
pub fn resolve_skill(name: &str, context: &SkillContext) -> SkillResolution {
    let own = agent_skills(&context.agent, &context.workspace, &context.home)
        .skills
        .into_iter()
        .find(|skill| skill.name == name);
    let call = own
        .as_ref()
        .and_then(|_| skill_source(&context.agent).and_then(|source| (source.call)(name)));
    if let Some(call) = call {
        return SkillResolution::Call(call);
    }
    let file = own.or_else(|| {
        let mut folders = vec![&context.workspace];
        if context.repo.as_os_str() != context.workspace.as_os_str() {
            folders.push(&context.repo);
        }
        folders.into_iter().find_map(|folder| {
            SKILL_SOURCES.iter().find_map(|(agent, _)| {
                agent_skills(agent, folder, &context.home)
                    .skills
                    .into_iter()
                    .find(|skill| skill.name == name)
            })
        })
    });
    match file {
        Some(skill) => SkillResolution::File(shown_path(&skill.path, &context.workspace)),
        None => SkillResolution::Missing,
    }
}

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
}

/// The length of `[\w.-]+` at the start of the text.
fn name_part(text: &[u8]) -> usize {
    text.iter().take_while(|byte| is_name_char(**byte)).count()
}

/// The length of `skill:<name>}` after the `{` and the name of a placeholder at the start of the
/// text: `\{skill:([\w.-]+(?::[\w.-]+)?)\}`.
fn placeholder(text: &str) -> Option<(usize, &str)> {
    let rest = text.strip_prefix("{skill:")?;
    let bytes = rest.as_bytes();
    let mut end = name_part(bytes);
    if end == 0 {
        return None;
    }
    if bytes.get(end) == Some(&b':') {
        let second = name_part(&bytes[end + 1..]);
        if second == 0 {
            return None;
        }
        end += 1 + second;
    }
    (bytes.get(end) == Some(&b'}')).then(|| ("{skill:".len() + end + 1, &rest[..end]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilledSkills {
    pub text: String,
    pub missing: Vec<String>,
}

/// Fills each `{skill:<name>}` of a prompt for the task's agent. The text around a placeholder
/// stays as it is, so each form reads well inside a sentence. Also the names that were missing,
/// each once.
pub fn fill_skills(text: &str, context: &SkillContext) -> FilledSkills {
    let mut missing: Vec<String> = Vec::new();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{skill:") {
        out.push_str(&rest[..start]);
        let here = &rest[start..];
        let Some((length, name)) = placeholder(here) else {
            out.push('{');
            rest = &here[1..];
            continue;
        };
        match resolve_skill(name, context) {
            SkillResolution::Call(call) => out.push_str(&call),
            SkillResolution::File(path) => {
                out.push_str(&format!(
                    "the skill in {path} (read that file and follow it)"
                ));
            }
            SkillResolution::Missing => {
                if !missing.iter().any(|each| each == name) {
                    missing.push(name.to_string());
                }
                out.push_str(&format!("the {name} skill"));
            }
        }
        rest = &here[length..];
    }
    out.push_str(rest);
    FilledSkills { text: out, missing }
}

/// A path inside the repository is shown from its root; one elsewhere in full.
pub fn shown_path(path: &Path, root: &Path) -> String {
    let inside = relative(root, path);
    if inside.is_empty() || inside.starts_with("..") || Path::new(&inside).is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    inside.replace('\\', "/")
}

#[cfg(test)]
#[path = "skill_resolve_tests.rs"]
mod tests;
