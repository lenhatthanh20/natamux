//! One stage of the Progress tab as a node of the flow (docs/design-task-stages.md 7.5,
//! `StageCard.tsx`): the bare node and the details card are in `flow_node.rs`; this file holds
//! what the details card of the selected node shows: the chips (output file, checks, send
//! backs, the PR), the reasons and wait boxes, the evidence, the workflow and CI boxes, and the
//! actions of the current stage.

use gpui::{AnyElement, Div, FocusHandle, FontWeight, Stateful};
use natamux_proto::stages::actions::StageFile;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use ui::prelude::*;

use super::card_rules::CardFacts;
use super::stage_choices::{ChoiceBox, choice_box};
use super::stage_view_rules::{finished_when, stage_time};
use crate::components::button_style::quiet_button;
use crate::tasks::details_commands::{OnCommand, PanelCommand};

/// The stage's output file as the card shows it.
#[derive(Clone, Debug)]
pub enum FileChip {
    Loading,
    Unreadable,
    Missing,
    Written(StageFile),
}

/// What one card shows; the Progress tab works it out.
pub struct CardInput {
    pub entry: StageProgress,
    pub stage: Stage,
    pub now_ms: i64,
    /// The selected node: a ring on the node, and its details card under the step.
    pub selected: bool,
    /// The agent icon of the node (`card_rules::node_agent`): the id, and true for the task's
    /// agent.
    pub node_agent: Option<(String, bool)>,
    pub file: Option<FileChip>,
    pub pr: Option<(String, bool)>,
    pub usage: Option<String>,
    pub activity: Option<String>,
    pub ci_branch: Option<String>,
    pub ran_workflow: bool,
    pub missing_skills: Vec<String>,
    pub skills_agent: String,
    /// The chip, buttons and notices (`card_rules`).
    pub facts: CardFacts,
    /// "Show evidence" is open.
    pub evidence_open: bool,
    /// The node takes the focus, so Enter and Space select it.
    pub fold_focus: Option<FocusHandle>,
    pub boxes: Vec<ChoiceBox>,
    /// The Skip / Don't skip of a stage not reached yet: (marked, busy).
    pub skip: Option<(bool, bool)>,
    pub actions: Option<ActionsInput>,
    /// A node of a side-by-side pair: smaller tile and name, the agent as its icon only.
    pub compact: bool,
}

/// The actions row of the current stage, while the task is open.
pub struct ActionsInput {
    pub busy: bool,
    pub resuming: bool,
    pub agent_label: String,
}

fn kilobytes(size: u64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else {
        format!("{} KB", (size as f64 / 102.4).round() / 10.)
    }
}

fn small_chip(id: impl Into<ElementId>, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    h_flex()
        .id(id.into())
        .h(rems_from_px(24.0_f32))
        .max_w_full()
        .min_w_0()
        .gap_1p5()
        .px_2()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.elevated_surface_background)
}

fn icon(name: &str) -> Icon {
    Icon::from_path(icon_path(name)).size(IconSize::Small)
}

fn badge(text: String, cx: &App) -> impl IntoElement {
    div()
        .px_1p5()
        .rounded_md()
        .bg(cx.theme().colors().element_background)
        .child(
            Label::new(text)
                .size(LabelSize::XSmall)
                .weight(FontWeight::SEMIBOLD)
                .color(Color::Muted),
        )
}

/// The facts of the node's meta line, in order: time, "you approve", who approved, cost and
/// tokens. The tooltip lists the same parts.
fn fact_parts(input: &CardInput) -> Vec<String> {
    let entry = &input.entry;
    let finished = matches!(entry.status, StageStatus::Done | StageStatus::Skipped);
    let manual = input.stage.gate == Gate::Manual && input.stage.kind != StageKind::Release;
    let approved = match (&entry.approved_by, entry.status) {
        (Some(source), StageStatus::Done) => Some(format!(
            "Approved by {}",
            if source == "human" { "you" } else { source }
        )),
        _ => None,
    };
    [
        stage_time(entry, input.now_ms),
        (manual && !finished).then(|| "you approve".to_owned()),
        approved,
        input.usage.clone(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Facts line, chips, notices, evidence and the boxes of an open card.
fn body(input: &CardInput, on: &OnCommand, cx: &App) -> Vec<AnyElement> {
    let mut parts = Vec::new();
    if let Some(activity) = &input.activity {
        parts.push(running_note("Now", activity, cx).into_any_element());
    }
    if let Some(branch) = &input.ci_branch {
        parts.push(running_note("Checking on CI", branch, cx).into_any_element());
    }
    let chips = chips(input, on, cx);
    if !chips.is_empty() {
        parts.push(
            h_flex()
                .flex_wrap()
                .gap_1p5()
                .children(chips)
                .into_any_element(),
        );
    }
    parts.extend(notices(input, cx));
    if !input.entry.passes.is_empty() {
        parts.push(evidence(input, on, cx).into_any_element());
    }
    for each in &input.boxes {
        parts.push(choice_box(each, on, cx).into_any_element());
    }
    if let Some(actions) = &input.actions {
        parts.push(actions_row(input, actions, on).into_any_element());
    }
    parts
}

#[path = "stage_chips.rs"]
mod chips;
use chips::{chips, evidence, notices, running_note};

#[path = "stage_actions.rs"]
mod actions;
use crate::components::icons::icon_path;
use actions::actions_row;

/// One bare node of the flow.
pub fn stage_card(input: CardInput, on: &OnCommand, cx: &App) -> AnyElement {
    node::node(input, on, cx)
}

/// The full-width details card of the selected node; its caret points `at` the node.
pub fn details_card(
    input: CardInput,
    at: super::flow_rules::CaretAt,
    on: &OnCommand,
    cx: &App,
) -> AnyElement {
    node::details(input, at, on, cx)
}

#[path = "flow_node.rs"]
mod node;

/// A stage not reached yet: what finishes it, its boxes, and Skip / Don't skip.
fn upcoming_parts(input: &CardInput, on: &OnCommand, cx: &App) -> Vec<AnyElement> {
    let stage_id = &input.entry.stage;
    let mut parts = vec![
        Label::new(finished_when(&input.stage))
            .size(LabelSize::Small)
            .color(Color::Muted)
            .into_any_element(),
    ];
    for each in &input.boxes {
        parts.push(choice_box(each, on, cx).into_any_element());
    }
    if let Some((marked, busy)) = input.skip {
        let label = if marked { "Don't skip" } else { "Skip" };
        let on = on.clone();
        let command = PanelCommand::Toggle {
            kind: crate::tasks::details_commands::StageToggle::Skip,
            stage: stage_id.clone(),
            on: !marked,
        };
        parts.push(
            quiet_button(SharedString::from(format!("skip-mark-{stage_id}")), label)
                .label_size(LabelSize::Small)
                .disabled(busy)
                .on_click(move |_, window, cx| on(command.clone(), window, cx))
                .into_any_element(),
        );
    }
    parts
}

#[cfg(test)]
#[path = "stage_card_tests.rs"]
mod tests;
