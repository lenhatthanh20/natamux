//! The plan limits of the agents as the status bar, its menu, the warning toast and the agent
//! picker show them. Rules only: the caller passes `now_ms`.

use std::collections::BTreeSet;

use natamux_proto::agents::limits::{AgentLimits, LimitWindow};
use natamux_proto::tasks::task::Task;

use super::agent_choices::agent_label;
use crate::tasks::task_time::parse_iso_ms;

/// A window at or above this share is a warning.
pub const WARN_PERCENT: f64 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warn,
    Full,
}

pub fn level(used_percent: f64) -> Level {
    if used_percent >= 100.0 {
        Level::Full
    } else if used_percent >= WARN_PERCENT {
        Level::Warn
    } else {
        Level::Normal
    }
}

fn percent(used: f64) -> String {
    format!("{}%", used.round().clamp(0.0, 100.0) as i64)
}

/// The windows that have not reset at `now_ms`: the bar may draw long after the limits came.
pub fn current(limits: &[AgentLimits], now_ms: i64) -> Vec<AgentLimits> {
    limits
        .iter()
        .map(|each| AgentLimits {
            windows: each
                .windows
                .iter()
                .filter(|window| {
                    let reset = window.resets_at.as_deref().and_then(parse_iso_ms);
                    reset.is_none_or(|reset| reset > now_ms)
                })
                .cloned()
                .collect(),
            ..each.clone()
        })
        .filter(|each| !each.windows.is_empty())
        .collect()
}

/// "in 2h 10m", "in 3d 4h", "in 5m". None without a reset time.
pub fn resets_in(resets_at: Option<&str>, now_ms: i64) -> Option<String> {
    let minutes = (parse_iso_ms(resets_at?)? - now_ms).max(0) / 60_000;
    let (days, hours, rest) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
    Some(match (days, hours) {
        (0, 0) => format!("in {}m", rest.max(1)),
        (0, _) => format!("in {hours}h {rest}m"),
        _ => format!("in {days}d {hours}h"),
    })
}

/// The first word of the agent's name, for the narrow places: "Claude", "Gemini".
pub fn short_name(agent: &str) -> String {
    let label = agent_label(Some(agent));
    label.split(' ').next().unwrap_or(&label).to_string()
}

/// One agent in the menu: its most used window first.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitRow {
    pub name: String,
    pub used: f64,
    pub level: Level,
    /// "84% 5h · 41% week"
    pub detail: String,
    /// "resets in 2h 10m", of the most used window.
    pub reset: String,
}

/// The agents that told their limits, the most used first.
pub fn limit_rows(limits: &[AgentLimits], now_ms: i64) -> Vec<LimitRow> {
    let mut rows: Vec<LimitRow> = limits
        .iter()
        .filter_map(|each| {
            let top = each.top()?;
            let detail = each
                .windows
                .iter()
                .map(|window| format!("{} {}", percent(window.used_percent), window.short_name()))
                .collect::<Vec<_>>()
                .join(" · ");
            let reset = resets_in(top.resets_at.as_deref(), now_ms)
                .map(|when| format!("resets {when}"))
                .unwrap_or_default();
            Some(LimitRow {
                name: agent_label(Some(&each.agent)),
                used: top.used_percent,
                level: level(top.used_percent),
                detail,
                reset,
            })
        })
        .collect();
    rows.sort_by(|a, b| b.used.total_cmp(&a.used));
    rows
}

/// The status bar item: the agent nearest a limit.
#[derive(Clone, Debug, PartialEq)]
pub struct Chip {
    /// "Claude 84% · 5h": the menu says when it resets, as the bar does not draw by the clock.
    pub text: String,
    /// "84%", for a narrow window.
    pub short: String,
    pub level: Level,
    pub used: f64,
}

pub fn limits_chip(limits: &[AgentLimits]) -> Option<Chip> {
    let (each, top) = limits
        .iter()
        .filter_map(|each| Some((each, each.top()?)))
        .max_by(|a, b| a.1.used_percent.total_cmp(&b.1.used_percent))?;
    let level = level(top.used_percent);
    let used = percent(top.used_percent);
    let name = short_name(&each.agent);
    Some(Chip {
        text: format!("{name} {used} · {}", top.short_name()),
        short: used,
        level,
        used: top.used_percent,
    })
}

const WEEK_MS: i64 = 7 * 86_400_000;

/// The agents of the tasks that changed in the last week.
pub fn agents_in_use(tasks: &[Task], now_ms: i64) -> BTreeSet<String> {
    tasks
        .iter()
        .filter(|task| parse_iso_ms(&task.updated_at).is_some_and(|at| now_ms - at < WEEK_MS))
        .filter_map(|task| task.agent.clone())
        .collect()
}

/// The agents the tasks use that told no limits, by name.
pub fn without_limits(agents_in_use: &BTreeSet<String>, limits: &[AgentLimits]) -> Vec<String> {
    let mut names: Vec<String> = agents_in_use
        .iter()
        .filter(|agent| !limits.iter().any(|each| &each.agent == *agent))
        .map(|agent| agent_label(Some(agent)))
        .collect();
    names.sort();
    names
}

/// The picker's note after an agent: "84% · 5h", "full", or None without limits.
pub fn picker_note(agent: &str, limits: &[AgentLimits]) -> Option<String> {
    let top = limits.iter().find(|each| each.agent == agent)?.top()?;
    Some(match level(top.used_percent) {
        Level::Full => "full".into(),
        _ => format!("{} · {}", percent(top.used_percent), top.short_name()),
    })
}

/// A window by agent, length and reset: a new reset is a new window to warn about.
fn window_key(agent: &str, window: &LimitWindow) -> String {
    let reset = window.resets_at.as_deref().unwrap_or("");
    format!("{agent}|{}|{reset}", window.minutes)
}

/// The windows at or above `WARN_PERCENT` that `warned` does not hold yet, added to it, with the
/// toast that tells them. None when no window crossed.
pub fn new_warning(warned: &mut BTreeSet<String>, limits: &[AgentLimits]) -> Option<String> {
    let mut crossed: Vec<(String, f64)> = Vec::new();
    for each in limits {
        for window in &each.windows {
            if window.used_percent >= WARN_PERCENT && warned.insert(window_key(&each.agent, window))
            {
                match crossed.iter_mut().find(|(agent, _)| *agent == each.agent) {
                    Some((_, used)) => *used = used.max(window.used_percent),
                    None => crossed.push((each.agent.clone(), window.used_percent)),
                }
            }
        }
    }
    match crossed.as_slice() {
        [] => None,
        [(agent, used)] if *used >= 100.0 => Some(format!(
            "{} reached its plan limit.",
            agent_label(Some(agent))
        )),
        [(agent, used)] => Some(format!(
            "{} used {} of its plan limit.",
            agent_label(Some(agent)),
            percent(*used)
        )),
        many => {
            let names: Vec<String> = many
                .iter()
                .map(|(agent, used)| format!("{} {}", agent_label(Some(agent)), percent(*used)))
                .collect();
            Some(format!(
                "{} agents are above {}% of their plan limit: {}.",
                many.len(),
                WARN_PERCENT as i64,
                names.join(", ")
            ))
        }
    }
}

/// Every window at or above `WARN_PERCENT`, as `new_warning` keys: the limits the app saw when it
/// opened are not news.
pub fn warned_already(limits: &[AgentLimits]) -> BTreeSet<String> {
    let mut warned = BTreeSet::new();
    let _ = new_warning(&mut warned, limits);
    warned
}

#[cfg(test)]
#[path = "limits_rules_tests.rs"]
mod tests;
