//! The draft of the Stages tab (`pipeline-editor/settings-json.ts`). The draft is the text of
//! `.natamux.json`; the tab reads `stages` from it, writes them back, and keeps every other key as
//! it is.
//!
//! Where it differs from the web: a stage is written with the keys of `Stage` in the order of the
//! struct, not the order of the file; a key of a stage that `Stage` does not know is dropped; and
//! a field of a wrong JSON type (or a condition with no `type`) is refused at read time with the
//! error of serde, since the typed steps cannot hold it.

use natamux_proto::projects::repo::TrackerSettings;
use natamux_proto::projects::settings_rules::other_settings_problem;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{GATES, STAGE_KINDS, Stage, StageBranch, StageGroup, Step};
use serde_json::{Map, Value};

pub type RawSettings = Map<String, Value>;

pub const MAX_ID: usize = 40;
pub const MAX_GROUP: usize = 4;
pub const MAX_PATHS: usize = 4;

/// What a project does when `tracker.createIssue` is left out.
pub const DEFAULT_CREATE_ISSUE: &str = "manual";

/// A readable draft: the settings as the file has them, and its steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub raw: RawSettings,
    pub steps: Vec<Step>,
}

/// `ID_RULE`: lower case words of letters and digits joined by "-".
pub fn fits_id_rule(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

pub fn format_settings(raw: &RawSettings) -> String {
    serde_json::to_string_pretty(raw).unwrap_or_default()
}

// An object compares by its keys, not their order (`Map` equality does that).
pub fn same_json(a: &Value, b: &Value) -> bool {
    a == b
}

/// Two drafts are the same when they hold the same settings, however they are formatted.
pub fn same_settings(a: &str, b: &str) -> bool {
    match (
        serde_json::from_str::<Value>(a),
        serde_json::from_str::<Value>(b),
    ) {
        (Ok(left), Ok(right)) => same_json(&left, &right),
        _ => a == b,
    }
}

fn is_stage_shape(value: &Value) -> bool {
    let Some(stage) = value.as_object() else {
        return false;
    };
    stage.get("id").is_some_and(Value::is_string)
        && stage
            .get("kind")
            .and_then(Value::as_str)
            .is_some_and(|kind| STAGE_KINDS.contains(&kind))
        && stage
            .get("gate")
            .and_then(Value::as_str)
            .is_some_and(|gate| GATES.contains(&gate))
}

fn stages_fit(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|stages| stages.iter().all(is_stage_shape))
}

fn is_path_shape(value: &Value) -> bool {
    value.as_object().is_some_and(|path| {
        path.get("id").is_some_and(Value::is_string) && stages_fit(path.get("stages"))
    })
}

fn kinds_text() -> String {
    STAGE_KINDS.join(", ")
}

/// One step of the list, or the message that says why it cannot be drawn.
fn read_step(index: usize, value: &Value) -> Result<Step, String> {
    let number = index + 1;
    let object = value.as_object();
    let fits = match object {
        Some(step) if step.contains_key("paths") => {
            let paths = step.get("paths").and_then(Value::as_array);
            let fits = step.get("id").is_some_and(Value::is_string)
                && paths.is_some_and(|paths| paths.iter().all(is_path_shape));
            if !fits {
                return Err(format!(
                    "stages, item {number}: a branch needs an id and paths, each with an id and a list of stages. A stage needs an id, a kind ({}) and a gate (manual or auto).",
                    kinds_text()
                ));
            }
            true
        }
        Some(step) if step.contains_key("parallel") => {
            step.get("id").is_some_and(Value::is_string) && stages_fit(step.get("parallel"))
        }
        _ => is_stage_shape(value),
    };
    if !fits {
        return Err(format!(
            "stages, item {number}: each stage needs an id, a kind ({}) and a gate (manual or auto).",
            kinds_text()
        ));
    }
    let typed = match object {
        Some(step) if step.contains_key("paths") => {
            serde_json::from_value::<StageBranch>(value.clone()).map(Step::Branch)
        }
        Some(step) if step.contains_key("parallel") => {
            serde_json::from_value::<StageGroup>(value.clone()).map(Step::Group)
        }
        _ => serde_json::from_value::<Stage>(value.clone()).map(Step::Stage),
    };
    typed.map_err(|error| format!("stages, item {number}: {error}"))
}

/// The steps the Stages tab can show. Wrong values of a rule are left for `pipeline_issues`, which
/// names them next to their stage; only a shape the list cannot draw is refused here.
fn read_steps(value: Option<&Value>) -> Result<Vec<Step>, String> {
    let Some(value) = value else {
        return Ok(default_pipeline());
    };
    let Some(list) = value.as_array() else {
        return Err("stages: give a list of stages, such as [].".to_string());
    };
    list.iter()
        .enumerate()
        .map(|(index, step)| read_step(index, step))
        .collect()
}

/// Reads the draft text. Not JSON, not an object, or settings outside `stages` that break the
/// rules: the person fixes it in the JSON tab.
pub fn parse_draft(text: &str) -> Result<Draft, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("This is not JSON: {error}"))?;
    let Value::Object(raw) = value else {
        return Err("The settings are one JSON object, such as {}.".to_string());
    };
    let steps = read_steps(raw.get("stages"))?;
    if let Some(problem) = other_settings_problem(&raw) {
        return Err(problem);
    }
    Ok(Draft { raw, steps })
}

fn without(raw: &RawSettings, skip: &str) -> RawSettings {
    raw.iter()
        .filter(|(key, _)| key.as_str() != skip)
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// The settings with these steps. A pipeline equal to the default is left out, so the project
/// follows later changes of the default. `stages` keeps its place among the keys.
pub fn with_steps(raw: &RawSettings, steps: &[Step]) -> RawSettings {
    if steps == default_pipeline().as_slice() {
        return without(raw, "stages");
    }
    let value = serde_json::to_value(steps).unwrap_or(Value::Null);
    if raw.contains_key("stages") {
        let mut next = raw.clone();
        next.insert("stages".to_string(), value);
        return next;
    }
    let mut next = RawSettings::new();
    next.insert("stages".to_string(), value);
    next.extend(raw.clone());
    next
}

/// The tracker of the draft, with the defaults for what it leaves out or cannot read.
pub fn tracker_of(raw: &RawSettings) -> TrackerSettings {
    let value = raw
        .get("tracker")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let mut wrapper = RawSettings::new();
    wrapper.insert("tracker".to_string(), value.clone());
    if other_settings_problem(&wrapper).is_some() {
        return TrackerSettings::default();
    }
    serde_json::from_value(value).unwrap_or_default()
}

/// The settings with this `tracker.createIssue`. The default is left out, like the default
/// pipeline, and so is a tracker with nothing left. `tracker` keeps its place among the keys.
pub fn with_create_issue(raw: &RawSettings, create_issue: &str) -> RawSettings {
    let old = raw
        .get("tracker")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut tracker = without(&old, "createIssue");
    if create_issue != DEFAULT_CREATE_ISSUE {
        tracker.insert(
            "createIssue".to_string(),
            Value::String(create_issue.to_string()),
        );
    }
    if tracker.is_empty() {
        return without(raw, "tracker");
    }
    let mut next = raw.clone();
    next.insert("tracker".to_string(), Value::Object(tracker));
    next
}

/// `agents.maxSendBacks` of the draft; None when it is left out or cannot be read (no limit).
pub fn max_send_backs_of(raw: &RawSettings) -> Option<usize> {
    let most = raw.get("agents")?.get("maxSendBacks")?.as_u64()?;
    usize::try_from(most).ok()
}

/// The settings with this `agents.maxSendBacks`; None leaves it out.
pub fn with_max_send_backs(raw: &RawSettings, most: Option<usize>) -> RawSettings {
    with_agents_key(raw, "maxSendBacks", most.map(Value::from))
}

/// `agents.pauseIdleAfterMinutes` of the draft; None when it is left out or cannot be read (the
/// default).
pub fn pause_idle_of(raw: &RawSettings) -> Option<u64> {
    raw.get("agents")?.get("pauseIdleAfterMinutes")?.as_u64()
}

/// The settings with this `agents.pauseIdleAfterMinutes`; None leaves it out.
pub fn with_pause_idle(raw: &RawSettings, minutes: Option<u64>) -> RawSettings {
    with_agents_key(raw, "pauseIdleAfterMinutes", minutes.map(Value::from))
}

/// The settings with this key of `agents`; None leaves it out, and an `agents` with nothing left
/// goes too. `agents` keeps its place among the keys.
fn with_agents_key(raw: &RawSettings, key: &str, value: Option<Value>) -> RawSettings {
    let old = raw
        .get("agents")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut agents = without(&old, key);
    if let Some(value) = value {
        agents.insert(key.to_string(), value);
    }
    if agents.is_empty() {
        return without(raw, "agents");
    }
    let mut next = raw.clone();
    next.insert("agents".to_string(), Value::Object(agents));
    next
}

/// The settings to save over a file that changed on disk: the keys the person changed since
/// `start` (the file as it was loaded) take the draft's value; every other key keeps the value of
/// the file now.
pub fn merge_changes(start: &RawSettings, draft: &RawSettings, now: &RawSettings) -> RawSettings {
    let changed: Vec<&String> = start
        .keys()
        .chain(draft.keys())
        .filter(|key| start.get(*key) != draft.get(*key))
        .collect();
    let mut merged = RawSettings::new();
    for (key, value) in now {
        if !changed.contains(&key) {
            merged.insert(key.clone(), value.clone());
        } else if let Some(mine) = draft.get(key) {
            merged.insert(key.clone(), mine.clone());
        }
    }
    for key in changed {
        if let Some(mine) = draft.get(key)
            && !merged.contains_key(key)
        {
            merged.insert(key.clone(), mine.clone());
        }
    }
    merged
}

#[cfg(test)]
#[path = "editor_draft_tests.rs"]
mod tests;
