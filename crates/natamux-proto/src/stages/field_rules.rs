//! The rules of one field of a stage, a path or a condition (`stageSchema`, `stageIdSchema` and
//! `conditionSchema` in `packages/core/src/stages/schema.ts`), as the messages zod gives.

use crate::stages::schema::{Condition, Stage};

const ID_MESSAGE: &str = "A stage id is lower case words joined by \"-\"";
const OUTPUT_MESSAGE: &str = "An output is a markdown file name, such as spec.md";

/// zod 4's text for a string that is too short after `trim()`.
pub(crate) fn too_small(min: usize) -> String {
    format!("Too small: expected string to have >={min} characters")
}

fn too_big(max: usize) -> String {
    format!("Too big: expected string to have <={max} characters")
}

/// `/^[\w-]+(\.[\w-]+)*\.md$/`.
fn is_markdown_file(text: &str) -> bool {
    let Some(stem) = text.strip_suffix(".md") else {
        return false;
    };
    stem.split('.').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    })
}

/// `stageIdSchema`: lower case words and digits joined by "-", up to 40 characters.
pub fn stage_id_problem(id: &str) -> Option<String> {
    let words = id.split('-').all(|word| {
        !word.is_empty()
            && word
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    });
    if !words {
        return Some(ID_MESSAGE.into());
    }
    (id.len() > 40).then(|| too_big(40))
}

/// A name of a stage, group, branch or path: 1 to 60 characters once trimmed.
pub(crate) fn name_problem(name: Option<&str>) -> Option<String> {
    let length = name?.trim().encode_utf16().count();
    if length == 0 {
        return Some(too_small(1));
    }
    (length > 60).then(|| too_big(60))
}

fn blank(text: Option<&str>) -> Option<String> {
    text.filter(|text| text.trim().is_empty())
        .map(|_| too_small(1))
}

/// The fields of `stageSchema` that a stage breaks, as (field in the file, message). Types cannot
/// be wrong here: a stage that was read has them right.
pub fn stage_problems(stage: &Stage) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    let mut add = |field: &'static str, problem: Option<String>| {
        if let Some(message) = problem {
            found.push((field, message));
        }
    };
    add("id", stage_id_problem(&stage.id));
    add("name", name_problem(stage.name.as_deref()));
    add("prompt", blank(stage.prompt.as_deref()));
    add(
        "output",
        stage
            .output
            .as_deref()
            .filter(|output| !is_markdown_file(output))
            .map(|_| OUTPUT_MESSAGE.into()),
    );
    for section in stage.sections.iter().flatten() {
        add("sections", blank(Some(section)));
    }
    add("check", blank(stage.check.as_deref()));
    add("command", blank(stage.command.as_deref()));
    add("doneWhen", blank(stage.done_when.as_deref()));
    add("agent", blank(stage.agent.as_deref()));
    add("args", blank(stage.args.as_deref()));
    found
}

/// `conditionSchema`: (field, message) for each field a condition breaks.
pub fn condition_problems(condition: &Condition) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    match condition {
        Condition::Output {
            file,
            section,
            contains,
        } => {
            if !is_markdown_file(file) {
                found.push((
                    "file",
                    "A condition file is a markdown file, such as plan.md".to_string(),
                ));
            }
            if let Some(message) = blank(section.as_deref()) {
                found.push(("section", message));
            }
            if contains.trim().is_empty() {
                found.push(("contains", "Give the text to look for".to_string()));
            }
        }
        Condition::Command { run } => {
            if run.trim().is_empty() {
                found.push(("run", "Give the command to run".to_string()));
            }
        }
    }
    found
}
