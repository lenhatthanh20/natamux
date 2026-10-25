//! Token and cost text (`features/tasks/usage-format.ts`) and the rows of the Usage section
//! (`UsageSection.tsx`). USD is an estimate at the API list price.

// Ported with their tests (spec D14); the details panel calls them.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::agents::activity::{TaskUsage, UsageRow};

use super::task_rules::stage_name;
use crate::agents::agent_choices::agent_label;

const UNITS: [(u64, &str); 4] = [
    (1_000, "K"),
    (1_000_000, "M"),
    (1_000_000_000, "B"),
    (1_000_000_000_000, "T"),
];

/// 950, 12.3K, 610K, 1.2M: compact, at most one decimal (`Intl` compact, en-US).
pub fn format_tokens(count: u64) -> String {
    if count < 1000 {
        return count.to_string();
    }
    let mut index = UNITS
        .iter()
        .rposition(|(size, _)| count >= *size)
        .unwrap_or(0);
    let tenths = |index: usize| (count * 10 + UNITS[index].0 / 2) / UNITS[index].0;
    let mut value = tenths(index);
    // 999,950 rounds up to 1000K, which reads 1M.
    if value >= 10_000 && index + 1 < UNITS.len() {
        index += 1;
        value = tenths(index);
    }
    let whole = value / 10;
    match value % 10 {
        0 => format!("{whole}{}", UNITS[index].1),
        decimal => format!("{whole}.{decimal}{}", UNITS[index].1),
    }
}

/// $0.84, <$0.01, or a dash when there is no price.
pub fn format_usd(cost: Option<f64>) -> String {
    let Some(cost) = cost else {
        return "—".into();
    };
    if cost > 0.0 && cost < 0.005 {
        return "<$0.01".into();
    }
    let text = format!("{cost:.2}");
    let (whole, cents) = text.split_once('.').unwrap_or((&text, "00"));
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("${grouped}.{cents}")
}

pub fn row_tokens(row: &UsageRow) -> u64 {
    row.input + row.cache_read + row.cache_write + row.output
}

/// "$1.84 est. · 2.4M tokens", or the tokens alone when no model has a price.
pub fn usage_line(tokens: u64, cost_usd: Option<f64>) -> String {
    let count = format!("{} tokens", format_tokens(tokens));
    match cost_usd {
        None => count,
        Some(_) => format!("{} est. · {count}", format_usd(cost_usd)),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StageUsage {
    pub tokens: u64,
    pub cost_usd: Option<f64>,
}

/// The stage's part of the task's usage. None when the stage used nothing.
pub fn stage_usage(usage: Option<&TaskUsage>, stage: &str) -> Option<StageUsage> {
    let rows: Vec<&UsageRow> = usage?
        .stages
        .iter()
        .filter(|row| row.stage == stage)
        .collect();
    if rows.is_empty() {
        return None;
    }
    let priced: Vec<f64> = rows.iter().filter_map(|row| row.cost_usd).collect();
    Some(StageUsage {
        tokens: rows.iter().map(|row| row_tokens(row)).sum(),
        cost_usd: (!priced.is_empty()).then(|| priced.iter().sum()),
    })
}

/// One line of the usage table, as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageCells {
    pub stage: String,
    pub agent: String,
    pub model: String,
    pub input: String,
    pub cache: String,
    pub output: String,
    pub cost: String,
    /// Set when the model has no price; the cost cell then shows a dash and is muted.
    pub cost_tip: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageSectionText {
    /// None when there are no rows: the section then says "No usage yet."
    pub total: Option<String>,
    pub rows: Vec<UsageCells>,
    pub footer: String,
}

pub const NO_USAGE: &str = "No usage yet.";

pub fn usage_cells(row: &UsageRow) -> UsageCells {
    UsageCells {
        stage: if row.stage.is_empty() {
            "No stage".into()
        } else {
            stage_name(&row.stage)
        },
        agent: agent_label(Some(&row.agent)),
        model: row.model.clone(),
        input: format_tokens(row.input),
        cache: format_tokens(row.cache_read + row.cache_write),
        output: format_tokens(row.output),
        cost: format_usd(row.cost_usd),
        cost_tip: row
            .cost_usd
            .is_none()
            .then(|| format!("No price for {}", row.model)),
    }
}

/// The Usage section, or None for a task that never had an agent and has no usage.
pub fn usage_section(
    task_agent: Option<&str>,
    usage: Option<&TaskUsage>,
) -> Option<UsageSectionText> {
    if task_agent.is_none() && usage.is_none() {
        return None;
    }
    let rows = usage.filter(|usage| !usage.stages.is_empty());
    // shortcut: looks at the task's agent only, not at a stage's own agent.
    let cursor = if task_agent == Some("cursor") {
        " Cursor does not report usage."
    } else {
        ""
    };
    Some(UsageSectionText {
        total: rows.map(|usage| usage_line(usage.tokens, usage.cost_usd)),
        rows: rows
            .map(|usage| usage.stages.iter().map(usage_cells).collect())
            .unwrap_or_default(),
        footer: format!(
            "Estimated at API list prices. A subscription is billed by its plan.{cursor}"
        ),
    })
}

#[cfg(test)]
#[path = "usage_rules_tests.rs"]
mod tests;
