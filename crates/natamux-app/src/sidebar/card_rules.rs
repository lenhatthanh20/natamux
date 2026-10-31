//! What a task card says, worked out with no gpui: the fold key, the waiting box, the stage chip,
//! the agent avatars and the pull request chip.

use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::{Task, TaskState};

use crate::sidebar::comet::comet_of;
use crate::sidebar::shell_rows::{ShellRow, task_glyph_state};
use crate::sidebar::sidebar_text::{card_line, pull_request_number, step_text};
use crate::sidebar::sidebar_tree::{
    Folds, RowTier, StepRef, is_fold_open, row_tier, stage_label, stage_name, steps_of, with_fold,
};

/// A card shows this many agent avatars; the rest are a "+N".
pub const MAX_AVATARS: usize = 3;

pub fn card_fold_key(task_id: &str) -> String {
    format!("task:{task_id}")
}

/// The open workspace unfolds by itself; a saved fold wins.
pub fn is_card_open(folds: &[(String, bool)], task_id: &str, holds_current: bool) -> bool {
    is_fold_open(folds, &card_fold_key(task_id), holds_current)
}

/// Opening a task unfolds its card, and the card of its parent when it is a subtask; the card of
/// the task it replaces in main folds: one open card at a time.
pub fn opened_folds(folds: &[(String, bool)], prev: Option<&str>, task: &Task) -> Folds {
    let folded = match prev.filter(|prev| *prev != task.id) {
        Some(prev) => with_fold(folds, &card_fold_key(prev), false),
        None => folds.to_vec(),
    };
    let next = with_fold(&folded, &card_fold_key(&task.id), true);
    match task.parent_id.as_deref() {
        Some(parent) => with_fold(&next, &card_fold_key(parent), true),
        None => next,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitButton {
    Answer,
    OpenTerminal,
    OpenTask,
}

impl WaitButton {
    pub fn label(self) -> &'static str {
        match self {
            WaitButton::Answer => "Answer",
            WaitButton::OpenTerminal => "Open terminal",
            WaitButton::OpenTask => "Open task",
        }
    }
}

/// The box of a card whose task needs the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub text: String,
    pub button: WaitButton,
}

pub fn waiting_of(task: &Task, events: Option<&[StoredEvent]>) -> Option<Waiting> {
    if row_tier(task.state) != RowTier::Attention {
        return None;
    }
    let text = card_line(task, events)
        .map(|line| {
            line.strip_prefix("The agent asks: ")
                .unwrap_or(&line)
                .to_owned()
        })
        .or_else(|| fallback_text(task))?;
    let button = match task.state {
        TaskState::NeedsInput => WaitButton::Answer,
        TaskState::Failed => WaitButton::OpenTerminal,
        _ => WaitButton::OpenTask,
    };
    Some(Waiting { text, button })
}

fn fallback_text(task: &Task) -> Option<String> {
    match task.state {
        TaskState::Failed => Some("The agent failed.".to_owned()),
        TaskState::Blocked if task.depends_on.is_empty() => Some("Blocked.".to_owned()),
        TaskState::Blocked => Some(format!("Waits for {}.", task.depends_on.join(", "))),
        _ => None,
    }
}

/// "3/5 Implement": the step and the stage after the id chip of an open card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageChip {
    pub step: Option<String>,
    pub stage: String,
}

/// The stage line of a closed card. None for a draft, a done or a cancelled task.
pub fn card_stage(task: &Task) -> Option<String> {
    stage_label(task).and(comet_of(task).stage)
}

pub fn stage_chip(task: &Task) -> Option<StageChip> {
    let stage = card_stage(task)?;
    let comet = comet_of(task);
    let current = comet
        .at
        .checked_sub(1)
        .and_then(|at| steps_of(task).into_iter().nth(at));
    let stage = match current {
        Some(StepRef::Group { parallel, .. }) => parallel
            .iter()
            .map(|id| stage_name(id))
            .collect::<Vec<_>>()
            .join(" ∥ "),
        _ => stage,
    };
    Some(StageChip {
        step: step_text(comet.at, comet.of),
        stage,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentAvatar {
    pub agent: String,
    pub state: Option<TaskState>,
}

/// One avatar per agent terminal; the task's own agent when no terminal is read yet.
pub fn card_agents(task: &Task, rows: &[ShellRow]) -> Vec<AgentAvatar> {
    let own: Vec<AgentAvatar> = rows
        .iter()
        .filter_map(|row| {
            Some(AgentAvatar {
                agent: row.agent.clone()?,
                state: row.state,
            })
        })
        .collect();
    if !own.is_empty() {
        return own;
    }
    task.agent
        .iter()
        .map(|agent| AgentAvatar {
            agent: agent.clone(),
            state: Some(task_glyph_state(task, rows)),
        })
        .collect()
}

pub fn more_agents(count: usize) -> Option<String> {
    (count > MAX_AVATARS).then(|| format!("+{}", count - MAX_AVATARS))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrChip {
    pub text: String,
    pub tone: Option<TaskState>,
}

pub fn pr_chip(task: &Task) -> Option<PrChip> {
    let url = task.pr.as_deref()?;
    let number = pull_request_number(url);
    let checks = task.pr_status.as_ref().map(|status| status.checks.as_str());
    let (mark, tone) = match checks {
        Some("passing") => (" ✓", Some(TaskState::Done)),
        Some("failing") => (" ✕", Some(TaskState::Failed)),
        Some("pending") => (" ●", Some(TaskState::Running)),
        _ => ("", None),
    };
    Some(PrChip {
        text: format!("{number}{mark}"),
        tone,
    })
}

#[cfg(test)]
#[path = "card_rules_tests.rs"]
mod tests;
