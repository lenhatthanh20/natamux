//! One stage card of the Progress tab (docs/design-task-stages.md 7.5, `StageCard.tsx`): the title
//! with its chip, the facts line, the chips (output file, checks, send backs, the PR), the
//! reasons and wait boxes, the evidence, the workflow and CI boxes, and the actions of the
//! current stage. A finished card is one line until opened; an upcoming one is dashed.

use gpui::{AnyElement, Div, FocusHandle, FontWeight, KeyDownEvent, Stateful};
use natamux_proto::stages::actions::StageFile;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::card_rules::{CardChip, CardFacts};
use super::stage_choices::{ChoiceBox, choice_box};
use super::stage_view_rules::{finished_when, stage_time};
use crate::components::chips::StateChip;
use crate::components::colors::state_colors;
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
    pub opened: bool,
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
    /// A finished card's fold row takes the focus, so Enter and Space open it.
    pub fold_focus: Option<FocusHandle>,
    pub boxes: Vec<ChoiceBox>,
    /// The Skip / Don't skip of a stage not reached yet: (marked, busy).
    pub skip: Option<(bool, bool)>,
    pub actions: Option<ActionsInput>,
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

fn title_row(input: &CardInput, cx: &App) -> impl IntoElement {
    let entry = &input.entry;
    let upcoming = entry.status == StageStatus::Upcoming;
    let marked = input.facts.chip == CardChip::WillBeSkipped;
    let chip = match input.facts.chip {
        CardChip::State(state) => StateChip::new(state).into_any_element(),
        CardChip::Done => text_chip("Done", TaskState::Done, cx).into_any_element(),
        CardChip::Skipped => text_chip("Skipped", TaskState::Draft, cx).into_any_element(),
        CardChip::WillBeSkipped => {
            text_chip("Will be skipped", TaskState::Draft, cx).into_any_element()
        }
        CardChip::NotStarted => text_chip("Not started", TaskState::Draft, cx).into_any_element(),
        CardChip::NotTaken => text_chip("Not taken", TaskState::Draft, cx).into_any_element(),
    };
    let name_color = if marked { Color::Muted } else { Color::Default };
    h_flex()
        .flex_wrap()
        .gap_x_2()
        .gap_y_1()
        .child(
            Label::new(entry.name.clone())
                .weight(FontWeight::SEMIBOLD)
                .color(name_color),
        )
        .child(chip)
        .when(entry.rounds > 1, |this| {
            this.child(badge(format!("\u{d7}{}", entry.rounds), cx))
        })
        .when(input.ran_workflow && !upcoming, |this| {
            this.child(badge("Workflow".into(), cx))
        })
}

/// A chip with the colours of `state` and a text of its own ("Done", "Not started").
pub fn text_chip(text: &'static str, state: TaskState, cx: &App) -> impl IntoElement {
    let colors = state_colors(state, cx);
    h_flex()
        .flex_none()
        .h(rems_from_px(20.0_f32))
        .px(rems_from_px(8.0_f32))
        .gap(rems_from_px(6.0_f32))
        .rounded_full()
        .bg(colors.bg)
        .child(
            div()
                .flex_none()
                .size(rems_from_px(6.0_f32))
                .rounded_full()
                .bg(colors.fg),
        )
        .child(
            Label::new(text)
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Custom(colors.fg)),
        )
}

fn fact_line(input: &CardInput) -> Option<impl IntoElement> {
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
    let facts: Vec<String> = [
        stage_time(entry, input.now_ms),
        (manual && !finished).then(|| "you approve".to_owned()),
        approved,
        input.usage.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!facts.is_empty()).then(|| {
        Label::new(facts.join(" \u{b7} "))
            .size(LabelSize::XSmall)
            .color(Color::Muted)
    })
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

/// The whole card.
pub fn stage_card(input: CardInput, on: &OnCommand, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let entry = &input.entry;
    if entry.status == StageStatus::NotTaken {
        return h_flex()
            .gap_2()
            .px_3()
            .py_2p5()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(colors.border)
            .child(
                Label::new(entry.name.clone())
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::Muted),
            )
            .child(text_chip("Not taken", TaskState::Draft, cx))
            .into_any_element();
    }
    let current = entry.status == StageStatus::Current;
    let upcoming = entry.status == StageStatus::Upcoming;
    let head = v_flex()
        .gap_0p5()
        .child(title_row(&input, cx))
        .children(fact_line(&input));
    let card = v_flex()
        .id(SharedString::from(format!("card-{}", entry.stage)))
        .gap_2p5()
        .px_3()
        .py_2p5()
        .rounded_md()
        .border_1()
        .when(upcoming, |this| {
            this.border_dashed().border_color(colors.border)
        })
        .when(!upcoming, |this| {
            this.bg(colors.elevated_surface_background)
                .border_color(if current {
                    colors.border_focused
                } else {
                    colors.border_variant
                })
        });
    match entry.status {
        StageStatus::Done | StageStatus::Skipped => finished_card(card, head, &input, on, cx),
        StageStatus::Upcoming => card
            .child(head)
            .children(upcoming_parts(&input, on, cx))
            .into_any_element(),
        _ => card
            .child(head)
            .children(body(&input, on, cx))
            .into_any_element(),
    }
}

/// A finished card: one line that opens, with the chevron.
fn finished_card(
    card: Stateful<Div>,
    head: Div,
    input: &CardInput,
    on: &OnCommand,
    cx: &App,
) -> AnyElement {
    let on_fold = on.clone();
    let fold = PanelCommand::FoldCard(input.entry.stage.clone());
    let chevron = if input.opened {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    };
    let (on_key, fold_key) = (on_fold.clone(), fold.clone());
    let line = h_flex()
        .id(SharedString::from(format!("fold-{}", input.entry.stage)))
        .items_start()
        .gap_1p5()
        .cursor_pointer()
        .when_some(input.fold_focus.as_ref(), |this, focus| {
            this.track_focus(focus)
        })
        .on_click(move |_, window, cx| on_fold(fold.clone(), window, cx))
        // A button of the web: Enter and Space open and fold it too.
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
                on_key(fold_key.clone(), window, cx);
            }
        })
        .child(
            Icon::new(chevron)
                .size(IconSize::XSmall)
                .color(Color::Muted),
        )
        .child(head);
    let parts = if input.opened {
        body(input, on, cx)
    } else {
        Vec::new()
    };
    card.child(line).children(parts).into_any_element()
}

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
            Button::new(SharedString::from(format!("skip-mark-{stage_id}")), label)
                .label_size(LabelSize::Small)
                .disabled(busy)
                .on_click(move |_, window, cx| on(command.clone(), window, cx))
                .into_any_element(),
        );
    }
    parts
}
