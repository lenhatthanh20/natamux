//! The choices of a stage card (`StageChoices.tsx`, `StartAgentControl.tsx`): "Run with a workflow"
//! and "Check on CI" with their hints, and the menu of agents to start.

use gpui::{Pixels, Point};
use ui::{ButtonLike, Checkbox, ContextMenu, Tooltip, prelude::*};

use crate::agents::agent_choices::{custom_agents, pick_list};
use crate::components::menu::MenuLayer;
use crate::tasks::details_commands::{OnCommand, PanelCommand, StageToggle};

const WORKFLOW_ONLY_CLAUDE: &str = "Only Claude Code can run a workflow.";
const WORKFLOW_HINT: &str = "More accurate, uses many more tokens.";
const WORKFLOW_LATER: &str =
    "More accurate, uses many more tokens. Takes effect the next time this stage starts.";
const CI_HINT: &str = "When the agent is done, its work is pushed to a natamux-ci branch, and the \
                       stage passes once CI passes. A failure goes back to the agent.";

/// One checkbox of a card.
#[derive(Clone, Debug)]
pub struct ChoiceBox {
    pub kind: StageToggle,
    pub stage: String,
    pub on: bool,
    pub busy: bool,
    /// The task's agent can run a workflow (always true for CI).
    pub allowed: bool,
    /// The stage started: the hint moves into a tooltip.
    pub started: bool,
}

fn hint_of(choice: &ChoiceBox) -> &'static str {
    match choice.kind {
        StageToggle::Workflow if !choice.allowed => WORKFLOW_ONLY_CLAUDE,
        StageToggle::Workflow if choice.started => WORKFLOW_LATER,
        StageToggle::Workflow => WORKFLOW_HINT,
        _ => CI_HINT,
    }
}

pub fn choice_box(choice: &ChoiceBox, on: &OnCommand, _: &App) -> impl IntoElement {
    let label = match choice.kind {
        StageToggle::Workflow => "Run with a workflow",
        _ => "Check on CI",
    };
    // A task on another agent may still untick a choice made before.
    let disabled = (!choice.allowed && !choice.on) || choice.busy;
    let hint = hint_of(choice);
    let inline = !choice.started || (choice.kind == StageToggle::Workflow && !choice.allowed);
    let command = PanelCommand::Toggle {
        kind: choice.kind,
        stage: choice.stage.clone(),
        on: !choice.on,
    };
    let on = on.clone();
    let id = SharedString::from(format!("{}-{}", choice.kind.name(), choice.stage));
    v_flex()
        .gap_0p5()
        .child(
            ButtonLike::new(id.clone())
                .disabled(disabled)
                .when(!inline, |this| this.tooltip(Tooltip::text(hint)))
                .on_click(move |_, window, cx| on(command.clone(), window, cx))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Checkbox::new(
                                SharedString::from(format!("{id}-box")),
                                choice.on.into(),
                            )
                            .disabled(disabled)
                            .visualization_only(true),
                        )
                        .child(Label::new(label).size(LabelSize::Small)),
                ),
        )
        .when(inline, |this| {
            this.child(Label::new(hint).size(LabelSize::XSmall).color(Color::Muted))
        })
}

/// The agents a card can start, in the pickers' order.
pub fn open_agent_menu(position: Point<Pixels>, on: OnCommand, window: &mut Window, cx: &mut App) {
    let agents = pick_list(&custom_agents());
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        agents.into_iter().fold(menu, |menu, (id, label)| {
            let on = on.clone();
            menu.entry(label, None, move |window, cx| {
                on(PanelCommand::PickAgent(id.clone()), window, cx)
            })
        })
    });
    MenuLayer::open(menu, position, window, cx);
}
