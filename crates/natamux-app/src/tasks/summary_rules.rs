//! What the summary strip at the top of the Progress tab shows (T-0258): the pull request or
//! "No PR yet" on the left, the estimated cost and the tokens on the right.

use natamux_proto::agents::activity::TaskUsage;
use natamux_proto::tasks::task::Task;

use super::table_rules::{
    ChecksTone, format_tokens, format_usd, pull_request_cell, pull_request_number,
};

pub const NO_PR: &str = "No PR yet";

/// The pull request side of the strip.
#[derive(Clone, Debug, PartialEq)]
pub struct PrFacts {
    pub url: String,
    /// "#569", or "Pull request" when the address has no number.
    pub label: String,
    /// "open · checks pass" and its tone; None until GitHub answered.
    pub status: Option<(String, ChecksTone)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SummaryFacts {
    /// None: the strip shows `NO_PR`.
    pub pr: Option<PrFacts>,
    /// "$4.12 est.", only when a model has a price.
    pub cost: Option<String>,
    /// "2.4M tok", only when the agent reported tokens.
    pub tokens: Option<String>,
}

pub fn summary_facts(task: &Task, usage: Option<&TaskUsage>) -> SummaryFacts {
    let pr = task.pr.as_ref().map(|url| PrFacts {
        url: url.clone(),
        label: pull_request_number(url),
        status: pull_request_cell(task).map(|(text, tone)| (text.replace(", ", " · "), tone)),
    });
    SummaryFacts {
        pr,
        cost: usage
            .and_then(|usage| usage.cost_usd)
            .map(|cost| format!("{} est.", format_usd(Some(cost)))),
        tokens: usage
            .filter(|usage| usage.tokens > 0)
            .map(|usage| format!("{} tok", format_tokens(usage.tokens))),
    }
}

#[cfg(test)]
#[path = "summary_rules_tests.rs"]
mod tests;
