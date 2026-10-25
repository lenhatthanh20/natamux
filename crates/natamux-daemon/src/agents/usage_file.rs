//! The usage file of a task (`<TASK>.usage.json`, as Node's `JSON.stringify` writes it) and
//! `taskUsage`, the answer built from it.

use natamux_agents::usage::prices::cost_of;
use natamux_agents::usage::{ModelUsage, UsageCounts, add_counts, zero_counts};
use serde_json::{Map, Value, json};

/// What was last counted of one session file, so the next read adds only what is new.
#[derive(Clone)]
pub(super) struct FileEntry {
    agent: String,
    models: ModelUsage,
}

/// `files`: by session file. `stages`: what each stage used, by stage id ("" for none), then
/// agent. Both keep their insertion order, as a JS object does.
#[derive(Clone, Default)]
pub(super) struct Stored {
    pub(super) files: Vec<(String, FileEntry)>,
    pub(super) stages: Vec<(String, Vec<(String, ModelUsage)>)>,
}

fn counts_copy(counts: &UsageCounts) -> UsageCounts {
    UsageCounts {
        input: counts.input,
        cache_read: counts.cache_read,
        cache_write: counts.cache_write,
        cache_write_1h: counts.cache_write_1h,
        output: counts.output,
    }
}

fn is_zero(counts: &UsageCounts) -> bool {
    counts.input == 0
        && counts.cache_read == 0
        && counts.cache_write == 0
        && counts.cache_write_1h == 0
        && counts.output == 0
}

/// What is in `now` and not yet in `before`, by model. Never below 0, as a file may start again.
fn new_usage(now: &ModelUsage, before: &ModelUsage) -> ModelUsage {
    let mut added = ModelUsage::default();
    for (model, counts) in &now.0 {
        let old = before.get(model).map_or_else(zero_counts, counts_copy);
        let change = UsageCounts {
            input: counts.input.saturating_sub(old.input),
            cache_read: counts.cache_read.saturating_sub(old.cache_read),
            cache_write: counts.cache_write.saturating_sub(old.cache_write),
            cache_write_1h: counts.cache_write_1h.saturating_sub(old.cache_write_1h),
            output: counts.output.saturating_sub(old.output),
        };
        if !is_zero(&change) {
            added.0.push((model.clone(), change));
        }
    }
    added
}

struct Row<'a> {
    stage: &'a str,
    agent: &'a str,
    model: &'a str,
    counts: &'a UsageCounts,
    cost: Option<f64>,
}

fn rows(stored: &Stored) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    for (stage, agents) in &stored.stages {
        for (agent, models) in agents {
            for (model, counts) in &models.0 {
                rows.push(Row {
                    stage,
                    agent,
                    model,
                    counts,
                    // The cost comes from the price table at each call, so a fixed price applies
                    // to old tasks too.
                    cost: cost_of(counts, model),
                });
            }
        }
    }
    rows
}

/// `taskUsage`: the tokens and the estimated cost of a task, with a row for each stage, agent
/// and model.
pub(super) fn task_usage(stored: &Stored) -> Value {
    let rows = rows(stored);
    let tokens: u64 = rows
        .iter()
        .map(|row| {
            row.counts.input + row.counts.cache_read + row.counts.cache_write + row.counts.output
        })
        .sum();
    let priced: Vec<f64> = rows.iter().filter_map(|row| row.cost).collect();
    let cost = if priced.is_empty() {
        Value::Null
    } else {
        json!(priced.iter().fold(0.0, |sum, cost| sum + cost))
    };
    let mut unpriced: Vec<&str> = Vec::new();
    for row in rows.iter().filter(|row| row.cost.is_none()) {
        if !unpriced.contains(&row.model) {
            unpriced.push(row.model);
        }
    }
    let stages: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "stage": row.stage,
                "agent": row.agent,
                "model": row.model,
                "input": row.counts.input,
                "cacheRead": row.counts.cache_read,
                "cacheWrite": row.counts.cache_write,
                "output": row.counts.output,
                "costUsd": row.cost.map_or(Value::Null, |cost| json!(cost)),
            })
        })
        .collect();
    json!({
        "tokens": tokens,
        "costUsd": cost,
        "unpricedModels": unpriced,
        "stages": stages,
    })
}

fn models_value(models: &ModelUsage) -> Value {
    Value::Object(
        models
            .0
            .iter()
            .map(|(model, counts)| {
                let counts = json!({
                    "input": counts.input,
                    "cacheRead": counts.cache_read,
                    "cacheWrite": counts.cache_write,
                    "cacheWrite1h": counts.cache_write_1h,
                    "output": counts.output,
                });
                (model.clone(), counts)
            })
            .collect::<Map<String, Value>>(),
    )
}

/// The file's content: what `JSON.stringify(stored)` writes in Node.
pub(super) fn stored_value(stored: &Stored) -> Value {
    let files: Map<String, Value> = stored
        .files
        .iter()
        .map(|(path, entry)| {
            let value = json!({ "agent": entry.agent, "models": models_value(&entry.models) });
            (path.clone(), value)
        })
        .collect();
    let stages: Map<String, Value> = stored
        .stages
        .iter()
        .map(|(stage, agents)| {
            let agents: Map<String, Value> = agents
                .iter()
                .map(|(agent, models)| (agent.clone(), models_value(models)))
                .collect();
            (stage.clone(), Value::Object(agents))
        })
        .collect();
    json!({ "files": files, "stages": stages })
}

/// `z.number().int().nonnegative()`
fn count_of(value: &Value) -> Option<u64> {
    if let Some(count) = value.as_u64() {
        return Some(count);
    }
    let number = value.as_f64()?;
    // Whole numbers below 2^53 are what a JS number holds exactly.
    (number.fract() == 0.0 && (0.0..9_007_199_254_740_992.0).contains(&number))
        .then_some(number as u64)
}

fn counts_from(value: &Value) -> Option<UsageCounts> {
    let map = value.as_object()?;
    Some(UsageCounts {
        input: count_of(map.get("input")?)?,
        cache_read: count_of(map.get("cacheRead")?)?,
        cache_write: count_of(map.get("cacheWrite")?)?,
        cache_write_1h: count_of(map.get("cacheWrite1h")?)?,
        output: count_of(map.get("output")?)?,
    })
}

fn models_from(value: &Value) -> Option<ModelUsage> {
    let mut models = ModelUsage::default();
    for (model, counts) in value.as_object()? {
        models.0.push((model.clone(), counts_from(counts)?));
    }
    Some(models)
}

/// `storedUsageSchema`; None for anything that does not fit it.
pub(super) fn stored_from(value: &Value) -> Option<Stored> {
    let map = value.as_object()?;
    let mut stored = Stored::default();
    for (path, entry) in map.get("files")?.as_object()? {
        let entry = entry.as_object()?;
        stored.files.push((
            path.clone(),
            FileEntry {
                agent: entry.get("agent")?.as_str()?.to_string(),
                models: models_from(entry.get("models")?)?,
            },
        ));
    }
    for (stage, agents) in map.get("stages")?.as_object()? {
        let mut by_agent = Vec::new();
        for (agent, models) in agents.as_object()? {
            by_agent.push((agent.clone(), models_from(models)?));
        }
        stored.stages.push((stage.clone(), by_agent));
    }
    Some(stored)
}

/// `^([A-Z]+)-(\d+)\.usage\.json$`: the task id of a usage file's name.
pub(super) fn task_id_of_file(name: &str) -> Option<&str> {
    let id = name.strip_suffix(".usage.json")?;
    let (letters, digits) = id.split_once('-')?;
    let ok = !letters.is_empty()
        && letters.bytes().all(|byte| byte.is_ascii_uppercase())
        && !digits.is_empty()
        && digits.bytes().all(|byte| byte.is_ascii_digit());
    ok.then_some(id)
}

pub(super) struct Seen {
    pub(super) task_id: String,
    pub(super) stage: String,
    pub(super) agent: String,
    pub(super) transcript: String,
}

/// Adds what is new in `now` to the stage of the turn. False when there is nothing new.
pub(super) fn apply_read(stored: &mut Stored, seen: &Seen, now: ModelUsage) -> bool {
    let before = stored
        .files
        .iter()
        .find(|(path, _)| *path == seen.transcript)
        .map(|(_, entry)| entry.models.clone())
        .unwrap_or_default();
    let added = new_usage(&now, &before);
    if added.is_empty() {
        return false;
    }
    let stage = match stored.stages.iter().position(|(id, _)| *id == seen.stage) {
        Some(at) => at,
        None => {
            stored.stages.push((seen.stage.clone(), Vec::new()));
            stored.stages.len() - 1
        }
    };
    let agents = &mut stored.stages[stage].1;
    let agent = match agents.iter().position(|(id, _)| *id == seen.agent) {
        Some(at) => at,
        None => {
            agents.push((seen.agent.clone(), ModelUsage::default()));
            agents.len() - 1
        }
    };
    for (model, counts) in &added.0 {
        add_counts(&mut agents[agent].1, model, counts);
    }
    let entry = FileEntry {
        agent: seen.agent.clone(),
        models: now,
    };
    match stored
        .files
        .iter()
        .position(|(path, _)| *path == seen.transcript)
    {
        Some(at) => stored.files[at].1 = entry,
        None => stored.files.push((seen.transcript.clone(), entry)),
    }
    true
}
