//! One stage as a node of the Progress flow (T-0257): a bare node (the kind tile with the state
//! glyph at its corner, the name, the agent icon) with no border or background, and the
//! full-width details card of the selected node, with a caret pointing up at it. A click, Enter or Space selects a node; the
//! selected node gets a bright ring. The node draws no ports or lines: the flow draws them on its
//! edges, so nothing here may clip (no `overflow_hidden` on the root) and the ring is absolute,
//! so it takes no room. Only the running glyph moves.

use gpui::{AnyElement, BoxShadow, Div, FontWeight, Hsla, KeyDownEvent, Stateful, point, px};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{Gate, StageKind};
use ui::{Tooltip, prelude::*};

use super::{CardInput, badge, body, fact_parts, upcoming_parts};
use crate::agents::agent_choices::agent_label;
use crate::components::agent_badge::AgentIcon;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::shape::ShapeSize;
use crate::stages::card_rules::CardChip;
use crate::stages::editor_labels::{StageRunner, model_of};
use crate::stages::flow_lines::Caret;
use crate::stages::flow_rules::{CARET_GAP, CaretAt, NodeTone, Stroke, node_tone};
use crate::stages::kind_icon::{kind_icon, waits_icon};
use crate::stages::preview_rules::agent_line_text;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::task_rules::state_label;

const TILE: f32 = 36.;
const TILE_RADIUS: f32 = 10.;
/// The agent badge on the tile's corner, and how far it pokes out past the tile.
const BADGE: f32 = 18.;
const BADGE_OUT: f32 = 6.;
const NODE_RADIUS: f32 = 9.;
/// The gap between the tile and the text.
const PAD: f32 = 12.;
/// The gap in a node of a side-by-side pair, so its name has room.
const PAD_COMPACT: f32 = 8.;
/// The space between the node's edge and its content, the same on every side.
pub(super) const INSET: f32 = 12.;
/// The head's clear border, which holds the keyboard focus ring.
const FOCUS: f32 = 2.;
/// The selection ring is 2 px wide, outside the node.
const RING: f32 = 2.;

/// The word of the meta line: the state of the current node, or what the node is.
pub(super) fn chip_word(chip: CardChip) -> Option<&'static str> {
    match chip {
        CardChip::State(state) => Some(state_label(state)),
        CardChip::Done => None,
        CardChip::Skipped => Some("Skipped"),
        CardChip::WillBeSkipped => Some("Will be skipped"),
        CardChip::NotStarted => Some("Not started"),
        CardChip::NotTaken => Some("Not taken"),
    }
}

/// The tooltip of a node: its name, then the word and every fact on a line of their own.
pub(super) fn tooltip_text(name: &str, word: Option<&str>, facts: &[String]) -> String {
    std::iter::once(name.to_owned())
        .chain(word.map(str::to_owned))
        .chain(facts.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The node's agent and what its tooltip or label says: the stage's own agent with its model,
/// or "Task's agent · <name>".
fn agent_text(input: &CardInput) -> Option<(String, String)> {
    let (agent, tasks) = input.node_agent.clone()?;
    let text = if tasks {
        format!("Task's agent \u{b7} {}", agent_label(Some(&agent)))
    } else {
        let runner = StageRunner {
            agent: Some(agent.clone()),
            model: model_of(input.stage.args.as_deref()),
        };
        agent_line_text(&runner, |agent| agent_label(Some(agent)))
    };
    Some((agent, text))
}

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

/// The agent's icon in a round badge on the tile's corner; its name is in the tooltip.
fn agent_badge(input: &CardInput, cx: &App) -> Option<AnyElement> {
    let (agent, text) = agent_text(input)?;
    let colors = cx.theme().colors();
    Some(
        div()
            .id(SharedString::from(format!("agent-{}", input.entry.stage)))
            .debug_selector(|| "node-agent".into())
            .absolute()
            .right(rems_from_px(-BADGE_OUT))
            .bottom(rems_from_px(-BADGE_OUT))
            .size(rems_from_px(BADGE))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.panel_background)
            .child(AgentIcon::new(Some(agent)))
            .tooltip(Tooltip::text(text))
            .into_any_element(),
    )
}

/// A soft glow in `color` around a lit tile or chip.
fn glow(color: Hsla, blur: f32) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color,
        offset: point(px(0.), px(0.)),
        blur_radius: px(blur),
        spread_radius: px(0.),
        inset: false,
    }]
}

/// The kind tile: lit and glowing in the state's colour once the stage ran or runs, dashed and
/// dark while it has not run, with the agent badge on its corner.
fn tile(input: &CardInput, tone: NodeTone, accent: Hsla, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let lit = tone.color_state().is_some();
    div()
        .id(SharedString::from(format!("tile-{}", input.entry.stage)))
        .debug_selector(|| "node-tile".into())
        .relative()
        .flex_none()
        .size(rems_from_px(TILE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rems_from_px(TILE_RADIUS))
        .border_1()
        .map(|this| {
            if lit {
                this.border_color(accent.opacity(0.55))
                    .bg(accent.opacity(0.14))
                    .shadow(glow(accent.opacity(0.35), 14.))
            } else {
                this.border_color(colors.border)
            }
        })
        .when(matches!(tone.border(), Stroke::Dashed), |this| {
            this.border_dashed()
        })
        .child(
            kind_icon(input.stage.kind)
                .size(IconSize::Medium)
                .color(if lit {
                    Color::Custom(accent)
                } else {
                    Color::Muted
                }),
        )
        .children(agent_badge(input, cx))
}

/// The word of the state chip under a node's name.
pub(super) fn node_chip_word(chip: CardChip) -> &'static str {
    match chip {
        CardChip::State(state) => state_label(state),
        CardChip::Done => "Done",
        CardChip::Skipped => "Skipped",
        CardChip::WillBeSkipped => "Will be skipped",
        CardChip::NotStarted => "Not yet",
        CardChip::NotTaken => "Not taken",
    }
}

/// The state chip under a node's name: a lit pill with a check (done) or the state glyph
/// (current), or a dashed pill with an empty dashed ring (not run yet).
fn state_chip(input: &CardInput, tone: NodeTone, accent: Hsla, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let lit = tone.color_state().is_some();
    let mark: AnyElement = match (input.facts.chip, &input.ci_branch) {
        (CardChip::Done, _) => Icon::new(IconName::Check)
            .size(IconSize::XSmall)
            .color(Color::Custom(accent))
            .into_any_element(),
        (CardChip::State(_), Some(_)) => Icon::new(IconName::GitBranch)
            .size(IconSize::XSmall)
            .color(Color::Custom(accent))
            .into_any_element(),
        (CardChip::State(state), None) => StateGlyph::new(
            SharedString::from(format!("glyph-{}", input.entry.stage)),
            Some(state),
        )
        .size(ShapeSize::Terminal)
        .into_any_element(),
        _ => div()
            .flex_none()
            .size(rems_from_px(10.0_f32))
            .rounded_full()
            .border_1()
            .border_dashed()
            .border_color(colors.text_muted)
            .into_any_element(),
    };
    h_flex()
        .debug_selector(|| "node-chip".into())
        .flex_none()
        .gap_1()
        .px_2()
        .h(rems_from_px(20.0_f32))
        .rounded_full()
        .border_1()
        .map(|this| {
            if lit {
                this.border_color(accent.opacity(0.4))
                    .bg(accent.opacity(0.12))
                    .shadow(glow(accent.opacity(0.18), 8.))
            } else {
                this.border_color(colors.border).border_dashed()
            }
        })
        .child(mark)
        .child(
            Label::new(node_chip_word(input.facts.chip))
                .size(LabelSize::Small)
                .color(if lit {
                    Color::Custom(accent)
                } else {
                    Color::Muted
                }),
        )
}

fn name_label(input: &CardInput, tone: NodeTone) -> Label {
    Label::new(input.entry.name.clone())
        .color(if tone.muted() {
            Color::Muted
        } else {
            Color::Default
        })
        .when(tone.struck(), |label| label.strikethrough())
        .truncate()
}

/// The name of a bare node. It clips instead of wrapping, so a narrow node keeps its height.
fn name_row(input: &CardInput, tone: NodeTone) -> Div {
    h_flex()
        .debug_selector(|| "node-name".into())
        .min_w_0()
        .overflow_hidden()
        .child(div().min_w_0().child(name_label(input, tone)))
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
            div()
                .min_w_0()
                .flex_shrink_0()
                .max_w(relative(0.7))
                .child(name_label(input, tone).weight(FontWeight::SEMIBOLD)),
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

/// The node: tile and name, centred as one group. A click, Enter and Space select it; on the
/// selected node they go back to the current stage.
fn header(
    input: &CardInput,
    tone: NodeTone,
    accent: Hsla,
    on: &OnCommand,
    cx: &App,
) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let entry = &input.entry;
    let word = chip_word(input.facts.chip);
    let facts = fact_parts(input);
    let tip = tooltip_text(&entry.name, word, &facts);
    let hover_bg = colors.ghost_element_hover;
    let ring = colors.border_focused;
    let texts = v_flex()
        .debug_selector(|| "node-texts".into())
        .min_w_0()
        .gap_1()
        .child(name_row(input, tone))
        // In a row of its own, so the chip keeps its width and does not stretch.
        .child(h_flex().child(state_chip(input, tone, accent, cx)));
    let on_click = on.clone();
    let select = PanelCommand::SelectStage(entry.stage.clone());
    let (on_key, select_key) = (on_click.clone(), select.clone());
    h_flex()
        .id(SharedString::from(format!("fold-{}", entry.stage)))
        .w_full()
        .items_center()
        .justify_center()
        .gap(rems_from_px(if input.compact { PAD_COMPACT } else { PAD }))
        // A clear border holds the keyboard focus ring without moving the content.
        .p(rems_from_px(INSET - FOCUS))
        .border_2()
        .border_color(gpui::transparent_black())
        .rounded(rems_from_px(NODE_RADIUS))
        .child(tile(input, tone, accent, cx))
        .child(texts)
        .tooltip(Tooltip::text(tip))
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .focus_visible(move |style| style.border_color(ring))
        .when_some(input.fold_focus.as_ref(), |this, focus| {
            this.track_focus(focus)
        })
        .on_click(move |_, window, cx| on_click(select.clone(), window, cx))
        // A button of the web: Enter and Space select it too.
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
                on_key(select_key.clone(), window, cx);
            }
        })
}

/// The bright ring of the selected node: absolute and outside the node, so it takes no room.
fn selection_ring(color: Hsla) -> Div {
    let reach = rems_from_px(-RING);
    div()
        .absolute()
        .top(reach)
        .left(reach)
        .right(reach)
        .bottom(reach)
        .rounded(rems_from_px(NODE_RADIUS + RING))
        .border_2()
        .border_color(color)
}

/// The tone and the accent colour of a node.
fn tone_of(input: &CardInput, cx: &App) -> (NodeTone, Hsla) {
    let entry = &input.entry;
    let marked = input.facts.chip == CardChip::WillBeSkipped;
    let tone = node_tone(entry.status, entry.state, marked, input.ci_branch.is_some());
    let accent = tone
        .color_state()
        .map_or(cx.theme().colors().text_muted, |state| {
            state_colors(state, cx).fg
        });
    (tone, accent)
}

/// One bare node: no border, no background. Width is the parent's.
pub(super) fn node(input: CardInput, on: &OnCommand, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let (tone, accent) = tone_of(&input, cx);
    let fade = tone.fade();
    let head = header(&input, tone, accent, on, cx);
    v_flex()
        .id(SharedString::from(format!("card-{}", input.entry.stage)))
        .debug_selector(|| "node-card".into())
        .relative()
        .w_full()
        .min_w_0()
        // A pair row stretches both nodes to one height, so their bottom ports line up.
        .flex_grow_1()
        .justify_center()
        .rounded(rems_from_px(NODE_RADIUS))
        .when(input.selected, |this| {
            this.bg(colors.ghost_element_selected)
                .child(selection_ring(colors.border_focused))
        })
        .when(fade < 1., |this| this.opacity(fade))
        .child(head)
        .into_any_element()
}

/// The details card of the selected node, at the full width of the flow, right under the node's
/// row with a caret pointing up at the node (mockup A): the name with its marks, the state and
/// facts, then the body of the stage (its actions while current). No connector runs between the
/// node and the card; the flow's line goes on from the card's bottom.
pub(super) fn details(input: CardInput, at: CaretAt, on: &OnCommand, cx: &App) -> AnyElement {
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
