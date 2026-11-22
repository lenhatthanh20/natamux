//! The actions of the current stage card (`StageActions` in `StageCard.tsx`): the buttons that
//! `card_rules::card_buttons` gives, Approve first and wide. Every button is off while an action
//! of the panel is on its way, so a double click sends one request; the clicked one spins.

use ui::prelude::*;

use super::{ActionsInput, CardInput};
use crate::components::button_style::{action_button, busy, button_icon, key_hint, quiet_button};
use crate::components::chips::tip_with_keys;
use crate::stages::card_rules::CardButton;
use crate::stages::stage_view_rules::finished_when;
use crate::tasks::details_commands::{OnCommand, PanelCommand};

/// Tests find a card button by `card-<button>`, such as `card-Skip`; only test builds wrap it.
#[cfg(test)]
fn findable(which: CardButton, button: Button) -> AnyElement {
    div()
        .debug_selector(|| format!("card-{which:?}"))
        .child(button)
        .into_any_element()
}

#[cfg(not(test))]
fn findable(_: CardButton, button: Button) -> AnyElement {
    button.into_any_element()
}

/// `running`: the command whose call runs now; its button spins.
fn button(
    base: Button,
    on: &OnCommand,
    command: PanelCommand,
    running: Option<&PanelCommand>,
) -> Button {
    let on = on.clone();
    busy(base, running == Some(&command))
        .label_size(LabelSize::Small)
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
                approve = Some(findable(*each, approve_button(input, *anyway, actions, on)))
            }
            other => row.push(row_button(input, *other, actions, on)),
        }
    }
    v_flex()
        .gap_2()
        .children(approve)
        .child(h_flex().flex_wrap().gap_2().children(row))
}

fn approve_button(
    input: &CardInput,
    anyway: bool,
    actions: &ActionsInput,
    on: &OnCommand,
) -> Button {
    let stage = &input.entry.stage;
    let label = if anyway { "Approve anyway" } else { "Approve" };
    let off = actions.busy;
    button(
        action_button(SharedString::from(format!("approve-{stage}")), label),
        on,
        PanelCommand::Approve(stage.clone()),
        actions.running.as_ref(),
    )
    .full_width()
    .disabled(off)
    // The key approves a review; needs input has no key.
    .when(!anyway, |this| {
        this.key_binding(key_hint("a", off))
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
    let off = actions.busy;
    let running = actions.running.as_ref();
    let act = |name: &str, label: &'static str| action_button(SharedString::from(id(name)), label);
    let quiet = |name: &str, label: &'static str| quiet_button(SharedString::from(id(name)), label);
    let button = match which {
        CardButton::AnswerInTerminal => button(
            act("answer", "Answer in terminal"),
            on,
            PanelCommand::AnswerInTerminal,
            running,
        ),
        CardButton::Resume => {
            let resume = button(quiet("resume", "Resume"), on, PanelCommand::Resume, None);
            busy(resume, actions.resuming)
        }
        CardButton::PickAgent => {
            return findable(which, agent_picker(stage, &actions.agent_label, off, on));
        }
        CardButton::StartAgent => button(
            act("start-agent", "Start agent"),
            on,
            PanelCommand::StartStageAgent,
            running,
        )
        .start_icon(button_icon("play"))
        .disabled(off),
        CardButton::MarkAsDone => button(
            act("report", "Mark as done"),
            on,
            PanelCommand::Report(stage.clone()),
            running,
        )
        .disabled(off),
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
            running,
        )
        .disabled(off),
        CardButton::Skip => {
            let skip = PanelCommand::Skip(stage.clone());
            button(quiet("skip", "Skip"), on, skip, running).disabled(off)
        }
        CardButton::SendBack => button(
            quiet("send-back", "Send back"),
            on,
            PanelCommand::SendBack(stage.clone()),
            running,
        )
        .disabled(off),
        CardButton::Approve { anyway } => approve_button(input, anyway, actions, on),
    };
    findable(which, button)
}

/// "Agent to start": a menu of the agents, the picked one shown.
fn agent_picker(stage: &str, label: &str, busy: bool, on: &OnCommand) -> Button {
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

#[cfg(test)]
#[path = "stage_actions_row_tests.rs"]
mod row_tests;
