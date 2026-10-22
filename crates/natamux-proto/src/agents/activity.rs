//! What the agents do now (`GET /activity`, `activity.changed`) and what they used
//! (`GET /usage`, `usage.changed`). `packages/core/src/agents/usage.ts` and the hook service of
//! the daemon.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The last line of one terminal, such as "Edit a.ts".
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct AgentActivity {
    pub text: String,
    #[serde(default)]
    pub at: String,
}

/// Terminal name to its line, for one task.
pub type TaskActivity = BTreeMap<String, AgentActivity>;

/// The body of `GET /activity`: task id to the lines of its terminals.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ActivityList {
    #[serde(default)]
    pub activity: BTreeMap<String, TaskActivity>,
}

/// The newest line of a task. ISO times sort as text; on a tie the first terminal wins, as in the
/// dashboard (`readTaskActivity`).
pub fn latest_line(lines: &TaskActivity) -> Option<&str> {
    let mut latest: Option<&AgentActivity> = None;
    for line in lines.values() {
        if latest.is_none_or(|current| line.at > current.at) {
            latest = Some(line);
        }
    }
    latest.map(|line| line.text.as_str())
}

/// One stage, agent and model of a task. `cost_usd` is None when the model has no price.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    #[serde(default)]
    pub stage: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub input: u64,
    #[serde(default)]
    pub cache_read: u64,
    #[serde(default)]
    pub cache_write: u64,
    #[serde(default)]
    pub output: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

/// What a task used. USD is an estimate at the API list price.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskUsage {
    #[serde(default)]
    pub tokens: u64,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub unpriced_models: Vec<String>,
    #[serde(default)]
    pub stages: Vec<UsageRow>,
}

/// The body of `GET /usage`: task id to its usage.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct UsageList {
    #[serde(default)]
    pub usage: BTreeMap<String, TaskUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_lines_of_each_terminal_and_picks_the_newest() {
        let list: ActivityList = serde_json::from_str(
            r#"{"activity":{"T-0001":{
                "1":{"text":"Edit a.ts","at":"2026-10-22T01:00:00.000Z"},
                "2":{"text":"Edit test-spec.md","at":"2026-10-22T01:00:05.000Z"}}}}"#,
        )
        .unwrap();
        let lines = &list.activity["T-0001"];
        assert_eq!(lines["1"].text, "Edit a.ts");
        assert_eq!(latest_line(lines), Some("Edit test-spec.md"));
    }

    #[test]
    fn a_task_with_no_lines_has_no_latest() {
        assert_eq!(latest_line(&TaskActivity::new()), None);
    }

    #[test]
    fn a_tie_keeps_the_first_terminal() {
        let lines: TaskActivity =
            serde_json::from_str(r#"{"1":{"text":"a","at":"t"},"2":{"text":"b","at":"t"}}"#)
                .unwrap();
        assert_eq!(latest_line(&lines), Some("a"));
    }

    #[test]
    fn reads_usage_with_an_unpriced_model_and_ignores_other_fields() {
        let list: UsageList = serde_json::from_str(
            r#"{"usage":{"T-0001":{"tokens":1500,"costUsd":null,"unpricedModels":["m"],
                "stages":[{"stage":"","agent":"cursor","model":"m","input":1000,"cacheRead":0,
                "cacheWrite":0,"output":500,"costUsd":null}],"later":1}}}"#,
        )
        .unwrap();
        let usage = &list.usage["T-0001"];
        assert_eq!(usage.tokens, 1500);
        assert_eq!(usage.cost_usd, None);
        assert_eq!(usage.stages[0].output, 500);
        let priced: TaskUsage =
            serde_json::from_str(r#"{"tokens":3,"costUsd":0.25,"unpricedModels":[],"stages":[]}"#)
                .unwrap();
        assert_eq!(priced.cost_usd, Some(0.25));
    }
}
