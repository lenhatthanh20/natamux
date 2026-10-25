//! The actions of the current stage card (`StageActions` in `StageCard.tsx`): the buttons that
//! `card_rules::card_buttons` gives, Approve first and wide. Every button is off while an action
//! of the panel is on its way, so a double click sends one request.

use ui::prelude::*;

use super::{ActionsInput, CardInput};
use crate::components::button_style::{action_button, button_icon, key_hint, quiet_button};
use crate::components::chips::tip_with_keys;
use crate::stages::card_rules::CardButton;
use crate::stages::stage_view_rules::finished_when;
use crate::tasks::details_commands::{OnCommand, PanelCommand};

fn button(base: Button, on: &OnCommand, command: PanelCommand) -> Button {
    let on = on.clone();
    base.label_size(LabelSize::Small)
        .on_click(move |_, window, cx| on(command.clone(), window, cx))
}

pub(super) fn actions_row(
    input: &CardInput,
    actions: &ActionsInput,
    on: &OnCommand,
) -> impl IntoElement {
    let mut approve = None;
    let mut row = Vec::new();
    for each in &input.facts.buttons {
        match each {
            CardButton::Approve { anyway } => {
                approve = Some(approve_button(input, *anyway, actions.busy, on))
            }
            other => row.push(row_button(input, *other, actions, on)),
        }
    }
    v_flex()
        .gap_2()
        .children(approve)
        .child(h_flex().flex_wrap().gap_1p5().children(row))
}

fn approve_button(input: &CardInput, anyway: bool, busy: bool, on: &OnCommand) -> Button {
    let stage = &input.entry.stage;
    let label = if anyway { "Approve anyway" } else { "Approve" };
    button(
        action_button(SharedString::from(format!("approve-{stage}")), label),
        on,
        PanelCommand::Approve(stage.clone()),
    )
    .full_width()
    .disabled(busy)
    // The key approves a review; needs input has no key.
    .when(!anyway, |this| {
        this.key_binding(key_hint("a", busy))
            .tooltip(tip_with_keys("Approve", "a"))
    })
}

fn row_button(
    input: &CardInput,
    which: CardButton,
    actions: &ActionsInput,
    on: &OnCommand,
) -> AnyElement {
    let stage = &input.entry.stage;
    let id = |name: &str| format!("{name}-{stage}");
    let busy = actions.busy;
    let act = |name: &str, label: &'static str| action_button(SharedString::from(id(name)), label);
    let quiet = |name: &str, label: &'static str| quiet_button(SharedString::from(id(name)), label);
    let button = match which {
        CardButton::AnswerInTerminal => button(
            act("answer", "Answer in terminal"),
            on,
            PanelCommand::AnswerInTerminal,
        ),
        CardButton::Resume => {
            button(quiet("resume", "Resume"), on, PanelCommand::Resume).disabled(actions.resuming)
        }
        CardButton::PickAgent => {
            return agent_picker(stage, &actions.agent_label, busy, on).into_any_element();
        }
        CardButton::StartAgent => button(
            act("start-agent", "Start agent"),
            on,
            PanelCommand::StartStageAgent,
        )
        .start_icon(button_icon("play"))
        .disabled(busy),
        CardButton::MarkAsDone => button(
            act("report", "Mark as done"),
            on,
            PanelCommand::Report(stage.clone()),
        )
        .disabled(busy),
        CardButton::FinishedWhen => {
            return Label::new(finished_when(&input.stage))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .into_any_element();
        }
        CardButton::Start => button(
            act("start", "Start"),
            on,
            PanelCommand::Start(stage.clone()),
        )
        .disabled(busy),
        CardButton::Skip => {
            button(quiet("skip", "Skip"), on, PanelCommand::Skip(stage.clone())).disabled(busy)
        }
        CardButton::SendBack => button(
            quiet("send-back", "Send back"),
            on,
            PanelCommand::SendBack(stage.clone()),
        )
        .disabled(busy),
        CardButton::Approve { anyway } => approve_button(input, anyway, busy, on),
    };
    button.into_any_element()
}

/// "Agent to start": a menu of the agents, the picked one shown.
fn agent_picker(stage: &str, label: &str, busy: bool, on: &OnCommand) -> impl IntoElement {
    let on = on.clone();
    Button::new(
        SharedString::from(format!("agent-pick-{stage}")),
        label.to_owned(),
    )
    .label_size(LabelSize::Small)
    .style(ButtonStyle::Outlined)
    .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::XSmall))
    .disabled(busy)
    .on_click(move |event, window, cx| {
        crate::stages::stage_choices::open_agent_menu(event.position(), on.clone(), window, cx)
    })
}
