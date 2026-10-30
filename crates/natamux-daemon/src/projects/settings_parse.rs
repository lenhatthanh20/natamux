//! `parseSettings` and `projectSettingsSchema.safeParse` of `project-settings.ts`: a settings file
//! or a PUT body in, the settings with their defaults and zod's error text out.

use natamux_proto::projects::repo::{ContextSource, ProjectSettings};
use natamux_proto::projects::settings_rules::other_settings_problem;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::stage_problems;
use natamux_proto::stages::pipeline_rules::{IssueLevel, pipeline_issues};
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use serde_json::{Map, Value, json};

use crate::projects::settings_trim::trim_settings;

pub const SETTINGS_FILE: &str = ".natamux.json";

/// The settings as zod hands them over, and the same as JSON in zod's key order.
pub struct Parsed {
    pub settings: ProjectSettings,
    pub json: Value,
}

/// `parseSettings(text)`: the error is the text of `SettingsInputError`.
pub fn parse_text(text: &str) -> Result<Parsed, String> {
    let raw: Value = serde_json::from_str(text)
        .map_err(|error| format!("{SETTINGS_FILE} is not JSON: {}", json_error(text, &error)))?;
    parse_settings(&raw)
}

/// V8's `JSON.parse` message where it is easy to know: nothing to read, or a first character that
/// starts no JSON value. shortcut: other broken JSON gets serde's wording, as V8 names the token at
/// the exact offset; copy V8 if a dashboard ever shows those.
fn json_error(text: &str, error: &serde_json::Error) -> String {
    let start = text.trim_start_matches([' ', '\t', '\n', '\r']);
    let Some(first) = start.chars().next() else {
        return "Unexpected end of JSON input".into();
    };
    if first == '{' || first == '[' || first == '"' || first == '-' || first.is_ascii_digit() {
        return error.to_string();
    }
    if ["true", "false", "null"]
        .iter()
        .any(|word| word.starts_with(first))
    {
        return error.to_string();
    }
    let shown: String = text.chars().take(10).collect();
    let dots = if text.chars().count() > 10 { "..." } else { "" };
    format!("Unexpected token '{first}', \"{shown}\"{dots} is not valid JSON")
}

/// `projectSettingsSchema.safeParse(raw)` worded as `describe`: "path: message" joined by "; ".
pub fn parse_settings(raw: &Value) -> Result<Parsed, String> {
    let Value::Object(map) = raw else {
        return Err(format!(
            "file: Invalid input: expected object, received {}",
            kind_of(raw)
        ));
    };
    let mut problems: Vec<String> = Vec::new();
    let stages = match map.get("stages") {
        None => default_pipeline(),
        Some(value) => match serde_json::from_value::<Vec<Step>>(value.clone()) {
            Ok(steps) => {
                problems.extend(stage_issues(&steps));
                steps
            }
            Err(_) => {
                problems.push(match value {
                    // shortcut: zod names the field and its rule inside the step (its union of
                    // stage, group and branch); this names the step only (docs/rust-phase4.md,
                    // Gate, "Where it differs"). Copy zod's words if a dashboard needs the field.
                    Value::Array(items) => items
                        .iter()
                        .position(|item| serde_json::from_value::<Step>(item.clone()).is_err())
                        .map_or_else(
                            || "stages: Invalid input".to_string(),
                            |index| format!("stages.{index}: Invalid input"),
                        ),
                    other => format!(
                        "stages: Invalid input: expected array, received {}",
                        kind_of(other)
                    ),
                });
                Vec::new()
            }
        },
    };
    problems.extend(other_settings_problem(map));
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let mut rest: Map<String, Value> = map.clone();
    rest.remove("stages");
    let mut settings = serde_json::from_value::<ProjectSettings>(Value::Object(rest))
        .map_err(|error| error.to_string())?;
    settings.stages = stages;
    trim_settings(&mut settings);
    let json = settings_json(&settings);
    Ok(Parsed { settings, json })
}

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Where each stage sits, as the zod path: itself, its lane in a group, itself in a path.
fn places(steps: &[Step]) -> Vec<(String, &Stage)> {
    let mut found = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        match step {
            Step::Stage(stage) => found.push((format!("stages.{index}"), stage)),
            Step::Group(group) => {
                for (lane, stage) in group.parallel.iter().enumerate() {
                    found.push((format!("stages.{index}.parallel.{lane}"), stage));
                }
            }
            Step::Branch(branch) => {
                for (p, path) in branch.paths.iter().enumerate() {
                    for (s, stage) in path.stages.iter().enumerate() {
                        found.push((format!("stages.{index}.paths.{p}.stages.{s}"), stage));
                    }
                }
            }
        }
    }
    found
}

/// The path of the step or stage that has this id; the whole pipeline for None.
fn locate(steps: &[Step], id: Option<&str>) -> String {
    let Some(id) = id else {
        return "stages".into();
    };
    if let Some(index) = steps.iter().position(|step| step.id() == id) {
        return format!("stages.{index}");
    }
    places(steps)
        .into_iter()
        .find(|(_, stage)| stage.id == id)
        .map_or_else(|| "stages".into(), |(path, _)| path)
}

/// The problems of `pipelineSchema`: the fields of each stage first (zod names them one by one),
/// then the rules of the whole pipeline.
fn stage_issues(steps: &[Step]) -> Vec<String> {
    let mut found = Vec::new();
    for (path, stage) in places(steps) {
        for (field, message) in stage_problems(stage) {
            found.push(format!("{path}.{field}: {message}"));
        }
    }
    if !found.is_empty() {
        return found;
    }
    // The catalog's "Give the command to run" is a screen hint; the schema does not have it.
    let catalog = |id: &str| {
        steps.iter().flat_map(Step::stages).any(|stage| {
            stage.id == id
                && stage.kind == StageKind::Testing
                && stage.command.as_deref().is_none_or(str::is_empty)
        })
    };
    pipeline_issues(steps)
        .into_iter()
        .filter(|issue| issue.level == IssueLevel::Error)
        .filter(|issue| {
            !(issue.message == "Give the command to run"
                && issue.stage.as_deref().is_some_and(catalog))
        })
        .map(|issue| {
            format!(
                "{}: {}",
                locate(steps, issue.stage.as_deref()),
                issue.message
            )
        })
        .collect()
}

fn source_json(source: &ContextSource) -> Value {
    let mut map = Map::new();
    map.insert("type".into(), json!(source.kind));
    let own = match source.kind.as_str() {
        "skill" => Some(("skill", &source.skill)),
        "mcp" => Some(("prompt", &source.prompt)),
        _ => None,
    };
    if let Some((field, value)) = own {
        map.insert("name".into(), json!(source.name));
        map.insert(field.into(), json!(value));
        if let Some(agent) = &source.agent {
            map.insert("agent".into(), json!(agent));
        }
    }
    Value::Object(map)
}

/// The settings as JSON in the key order zod outputs (`projectSettingsSchema`).
pub fn settings_json(settings: &ProjectSettings) -> Value {
    let stages =
        serde_json::to_value(&settings.stages).unwrap_or_else(|_| Value::Array(Vec::new()));
    let sources: Vec<Value> = settings.context.sources.iter().map(source_json).collect();
    let mut agents = Map::new();
    agents.insert(
        "permissionMode".into(),
        json!(settings.agents.permission_mode),
    );
    // zod leaves an optional key out when the file does.
    if let Some(most) = settings.agents.max_send_backs {
        agents.insert("maxSendBacks".into(), json!(most));
    }
    if let Some(minutes) = settings.agents.pause_idle_after_minutes {
        agents.insert("pauseIdleAfterMinutes".into(), json!(minutes));
    }
    let mut json = json!({
        "stages": stages,
        "context": { "sources": sources },
        "tracker": {
            "type": settings.tracker.kind,
            "createIssue": settings.tracker.create_issue,
            "linkInPr": settings.tracker.link_in_pr,
        },
        "agents": agents,
    });
    if let (Some(name), Some(map)) = (&settings.git.branch_name, json.as_object_mut()) {
        map.insert("git".into(), json!({ "branchName": name }));
    }
    json
}

/// The settings of a missing or broken file: `DEFAULT_PROJECT_SETTINGS`.
pub fn default_settings() -> ProjectSettings {
    ProjectSettings {
        stages: default_pipeline(),
        ..ProjectSettings::default()
    }
}
