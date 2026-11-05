//! Words and small rules of the stage card list (layout-b 5.2): a row's tile colour and meta
//! text, the captions of groups and paths, the stage count, and which stage menu items are on.

use natamux_proto::stages::schema::{
    BranchPath, Condition, Stage, StageBranch, StageGroup, StageKind, Step,
};

use super::editor_catalog::is_custom;
use super::editor_find::{group_of, runs_in_group};
use super::editor_moves::{can_move, join_problem};
use super::preview_rules::waits_for_person;
use crate::agents::agent_choices::agent_label;
use crate::components::colors::TileColor;

/// A stage of your own is cyan; else by kind: testing green, manual yellow, release violet,
/// the agent kinds blue.
pub fn kind_tile(stage: &Stage) -> TileColor {
    if is_custom(stage) {
        return TileColor::Cyan;
    }
    match stage.kind {
        StageKind::Testing => TileColor::Green,
        StageKind::Manual => TileColor::Yellow,
        StageKind::Release => TileColor::Violet,
        StageKind::Context | StageKind::Agent | StageKind::Review => TileColor::Blue,
    }
}

/// The Lucide glyph of a stage's tile in the list and the sheets (layout-b 5.2.1); the flowchart
/// keeps `kind_icon`.
pub fn kind_tile_icon(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Testing => "flask-conical",
        StageKind::Manual => "hand",
        StageKind::Release => "send",
        StageKind::Context | StageKind::Agent | StageKind::Review => "bot",
    }
}

/// "Codex · Auto", "pnpm test · Auto", "Waits for you": the agent when it is not the task's, the
/// command of a testing stage, then whether it waits.
pub fn row_meta(stage: &Stage) -> String {
    let mut parts = Vec::new();
    if let Some(agent) = stage.agent.as_deref().filter(|agent| !agent.is_empty()) {
        parts.push(agent_label(Some(agent)));
    }
    if stage.kind == StageKind::Testing
        && let Some(command) = stage.command.as_deref().filter(|text| !text.is_empty())
    {
        parts.push(command.to_owned());
    }
    let waits = if waits_for_person(stage) {
        "Waits for you"
    } else {
        "Auto"
    };
    parts.push(waits.to_owned());
    parts.join(" \u{b7} ")
}

/// "Review side by side", or "Side by side" for a group without a name.
pub fn group_caption(group: &StageGroup) -> String {
    match group
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        Some(name) => format!("{name} side by side"),
        None => "Side by side".to_owned(),
    }
}

/// "2 paths, checked in order".
pub fn branch_meta(branch: &StageBranch) -> String {
    let count = branch.paths.len();
    let noun = if count == 1 { "path" } else { "paths" };
    format!("{count} {noun}, checked in order")
}

/// What a path's caption says after its name.
pub fn path_condition(path: &BranchPath) -> String {
    let not = path.not == Some(true);
    match &path.when {
        None => "Otherwise".to_owned(),
        Some(Condition::Command { run }) => {
            let verb = if not { "does not succeed" } else { "succeeds" };
            format!("if {run} {verb}")
        }
        Some(Condition::Output {
            file,
            section,
            contains,
        }) => {
            let place = match section.as_deref().filter(|section| !section.is_empty()) {
                Some(section) => format!("{file} \u{203a} {section}"),
                None => file.clone(),
            };
            let verb = if not { "does not have" } else { "has" };
            format!("if {place} {verb} \"{contains}\"")
        }
    }
}

/// Every stage of the pipeline, also those in groups and paths.
pub fn stage_count(steps: &[Step]) -> usize {
    steps.iter().map(|step| step.stages().len()).sum()
}

/// The join item of a stage menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinItem {
    /// "Run on its own": the stage is in a group.
    Alone,
    /// "Run alongside the stage above", off with the reason when it cannot.
    Alongside(Option<String>),
    /// No join item: the stage cannot run in a group.
    None,
}

/// Which items of a stage's "..." menu are on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageMenu {
    pub up: bool,
    pub down: bool,
    pub join: JoinItem,
}

pub fn stage_menu(steps: &[Step], stage: &Stage) -> StageMenu {
    let id = stage.id.as_str();
    let join = if group_of(steps, id).is_some() {
        JoinItem::Alone
    } else if runs_in_group(stage.kind) {
        JoinItem::Alongside(join_problem(steps, id))
    } else {
        JoinItem::None
    };
    StageMenu {
        up: can_move(steps, id, -1),
        down: can_move(steps, id, 1),
        join,
    }
}

#[cfg(test)]
#[path = "list_rules_tests.rs"]
mod tests;
