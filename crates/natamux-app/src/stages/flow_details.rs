//! The details card of the selected node of the Progress flow (T-0257): the name with its marks,
//! the state and facts, then the body of the stage, at the full width of the flow with a caret
//! pointing up at the node.

use gpui::{AnyElement, Div, FontWeight, Hsla};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{Gate, StageKind};
use ui::{Tooltip, prelude::*};

use super::super::{CardInput, badge, body, fact_parts, upcoming_parts};
use super::{NODE_RADIUS, agent_text, chip_word, name_label, tone_of};
use crate::components::agent_badge::AgentIcon;
use crate::stages::flow_lines::Caret;
use crate::stages::flow_rules::{CARET_GAP, CaretAt, INSET, NodeTone};
use crate::stages::kind_icon::waits_icon;
use crate::tasks::details_commands::OnCommand;

/// The agent with its name, on the details card.
fn agent_chip(input: &CardInput) -> Option<AnyElement> {
    let (agent, text) = agent_text(input)?;
    Some(
        // Shrinks before the name: in a narrow row the label goes first, the icon stays.
        h_flex()
            .min_w_0()
            .flex_shrink_1()
            .overflow_hidden()
            .gap_1()
            .child(div().flex_none().child(AgentIcon::new(Some(agent))))
            .child(
                Label::new(text)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            )
            .into_any_element(),
    )
}

/// The name with its marks, on the details card: agent with its name, the person mark of a
/// manual gate, rounds and the workflow.
fn details_name_row(input: &CardInput, tone: NodeTone, cx: &App) -> Div {
    let entry = &input.entry;
    let manual = input.stage.gate == Gate::Manual && input.stage.kind != StageKind::Release;
    h_flex()
        .min_w_0()
        .gap_1p5()
        .overflow_hidden()
        // The name keeps its width up to 70%, so the agent chip gives way first.
        .child(
            div().min_w_0().flex_shrink_0().max_w(relative(0.7)).child(
                name_label(input, tone)
                    .weight(FontWeight::SEMIBOLD)
                    .truncate(),
            ),
        )
        .children(agent_chip(input))
        .when(manual, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!("gate-{}", entry.stage)))
                    .flex_none()
                    .child(waits_icon().size(IconSize::XSmall).color(Color::Muted))
                    .tooltip(Tooltip::text("You approve")),
            )
        })
        .when(entry.rounds > 1, |this| {
            let rounds = badge(format!("\u{d7}{}", entry.rounds), cx);
            this.child(div().flex_none().child(rounds))
        })
        .when(
            input.ran_workflow && entry.status != StageStatus::Upcoming,
            |this| this.child(div().flex_none().child(badge("Workflow".into(), cx))),
        )
}

/// The state word (in the state's colour on the current node), then the facts.
fn meta_line(word: Option<&str>, facts: &[String], tone: NodeTone, accent: Hsla) -> Option<Div> {
    if word.is_none() && facts.is_empty() {
        return None;
    }
    let colour = if tone.glow() {
        Color::Custom(accent)
    } else {
        Color::Muted
    };
    let joined = facts.join(" \u{b7} ");
    let facts_text = match (word.is_some(), joined.is_empty()) {
        (_, true) => None,
        (true, false) => Some(format!("\u{b7} {joined}")),
        (false, false) => Some(joined),
    };
    Some(
        h_flex()
            .debug_selector(|| "node-meta".into())
            .min_w_0()
            .gap_1()
            .overflow_hidden()
            .children(word.map(|word| {
                let label = Label::new(word.to_owned())
                    .size(LabelSize::XSmall)
                    .color(colour);
                div().flex_none().child(label)
            }))
            .children(facts_text.map(|text| {
                div().min_w_0().flex_shrink_1().child(
                    Label::new(text)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                )
            })),
    )
}

/// The details card of the selected node, at the full width of the flow, right under the node's
/// row with a caret pointing up at the node (mockup A): the name with its marks, the state and
/// facts, then the body of the stage (its actions while current). No connector runs between the
/// node and the card; the flow's line goes on from the card's bottom.
pub(crate) fn details(input: CardInput, at: CaretAt, on: &OnCommand, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let (tone, accent) = tone_of(&input, cx);
    let word = chip_word(input.facts.chip);
    let facts = fact_parts(&input);
    let parts = if input.entry.status == StageStatus::Upcoming {
        upcoming_parts(&input, on, cx)
    } else {
        body(&input, on, cx)
    };
    let head = v_flex()
        .w_full()
        .min_w_0()
        .gap(rems_from_px(1.0_f32))
        .child(details_name_row(&input, tone, cx))
        .children(meta_line(word, &facts, tone, accent));
    let card = v_flex()
        .id(SharedString::from(format!("details-{}", input.entry.stage)))
        .debug_selector(|| "node-details".into())
        .relative()
        .w_full()
        .min_w_0()
        .gap_2()
        .p(rems_from_px(INSET))
        .rounded(rems_from_px(NODE_RADIUS))
        .border_1()
        // The stronger border, so the card and its caret stand out on the canvas as in mockup A.
        .border_color(colors.border)
        .bg(colors.elevated_surface_background)
        .child(Caret::new(at))
        .child(head)
        .children(parts);
    div()
        .w_full()
        .pt(rems_from_px(CARET_GAP))
        .child(card)
        .into_any_element()
}
