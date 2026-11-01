//! One stage as a node of the Progress flow (T-0257, T-0259): a bare node (the kind tile with the
//! agent badge at its corner, the name, the state chip) with no border or background, and the
//! full-width details card of the selected node, with a caret pointing up at it. The tile sits on
//! the flow's line: in the middle of the row with the name at its left and the chip at its right,
//! or near the middle of a pair with the texts outside. The node draws the line's stubs above and
//! below its tile, so the lines touch it whatever the name's height; nothing here may clip (no
//! `overflow_hidden` on the root). A click, Enter or Space selects a node; the selected node gets
//! a bright ring, absolute, so it takes no room. Only the running glyph and the running ring move.

use gpui::{AnyElement, BoxShadow, Div, Hsla, KeyDownEvent, Stateful, point, px};
use natamux_proto::tasks::task::TaskState;
use ui::{Tooltip, prelude::*};

use super::{CardInput, fact_parts};
use crate::agents::agent_choices::agent_label;
use crate::components::agent_badge::AgentIcon;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::motion;
use crate::components::motion_rules::{Moving, pulse};
use crate::components::shape::ShapeSize;
use crate::stages::card_rules::CardChip;
use crate::stages::editor_labels::{StageRunner, model_of};
use crate::stages::flow_lines::FlowLink;
use crate::stages::flow_rules::{
    INSET, LinkStyle, NodeSide, NodeTone, RUN_RING_OUT, STUB, Stroke, TILE, TILE_BOX, node_tone,
};
use crate::stages::kind_icon::kind_icon;
use crate::stages::preview_rules::agent_line_text;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::task_rules::state_label;

const TILE_RADIUS: f32 = 10.;
/// The agent badge on the tile's corner, and how far it pokes out past the tile.
const BADGE: f32 = 18.;
const BADGE_OUT: f32 = 6.;
const NODE_RADIUS: f32 = 9.;
/// The gap between the tile column and the texts.
const PAD: f32 = 12.;
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
}

/// The name of a bare node. A long name wraps onto more lines (owner's picture, T-0259); the
/// tile's stubs grow with it.
fn name_row(input: &CardInput, tone: NodeTone) -> Div {
    div()
        .debug_selector(|| "node-name".into())
        .min_w_0()
        .max_w_full()
        .child(name_label(input, tone))
}

/// A line inside the node, `None` an empty spacer of the same size.
fn stub(style: Option<LinkStyle>, up: f32, down: f32) -> AnyElement {
    match style {
        Some(style) => FlowLink::stub(style, up, down).into_any_element(),
        None => div().flex_1().min_h(rems_from_px(STUB)).into_any_element(),
    }
}

/// The running ring around the tile: the Running colour, pulsing on the shared motion clock
/// (still when the window has no focus and in shots).
#[derive(IntoElement)]
struct RunningRing {
    accent: Hsla,
    tile: Stateful<Div>,
}

impl RenderOnce for RunningRing {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let opacity = pulse(motion::frame(Moving::Pulse, window, cx));
        let ring = div()
            .debug_selector(|| "node-run-ring".into())
            .absolute()
            .inset_0()
            .rounded(rems_from_px(TILE_RADIUS + RUN_RING_OUT))
            .border_2()
            .border_color(self.accent)
            .shadow(glow(self.accent.opacity(0.35), 14.))
            .opacity(opacity);
        // The ring first, so the tile and its agent badge draw over it.
        div()
            .relative()
            .flex_none()
            .size(rems_from_px(TILE_BOX))
            .flex()
            .items_center()
            .justify_center()
            .child(ring)
            .child(self.tile)
    }
}

/// The tile on the flow's line: the stub into it, the tile (in its ring while running), the stub
/// out of it. As wide as the ring on every node, so a pair's tiles keep their x. The stubs reach
/// over the head's clear border, so they meet the flow's lines.
fn tile_column(input: &CardInput, tone: NodeTone, accent: Hsla, cx: &App) -> Div {
    let tile = tile(input, tone, accent, cx);
    let running = matches!(
        tone,
        NodeTone::Current {
            state: TaskState::Running,
            ..
        }
    );
    let middle = if running {
        RunningRing { accent, tile }.into_any_element()
    } else {
        tile.into_any_element()
    };
    v_flex()
        .debug_selector(|| "node-tile-column".into())
        .flex_none()
        .w(rems_from_px(TILE_BOX))
        .items_center()
        .child(stub(input.links.into, FOCUS, 0.))
        .child(middle)
        .child(stub(input.links.out, 0., FOCUS))
}

/// The name over the chip, at one side of the tile in a pair.
fn texts(input: &CardInput, tone: NodeTone, accent: Hsla, cx: &App) -> Div {
    v_flex()
        .debug_selector(|| "node-texts".into())
        .flex_1()
        .min_w_0()
        .justify_center()
        .gap_1()
        .child(name_row(input, tone))
        .child(state_chip(input, tone, accent, cx))
}

/// The node: the tile column on the flow's line, the texts around it (`NodeSide`). A click,
/// Enter and Space select it; on the selected node they go back to the current stage.
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
    let column = tile_column(input, tone, accent, cx);
    let on_click = on.clone();
    let select = PanelCommand::SelectStage(entry.stage.clone());
    let (on_key, select_key) = (on_click.clone(), select.clone());
    let row = h_flex()
        .id(SharedString::from(format!("fold-{}", entry.stage)))
        .w_full()
        // Fills a node that a pair row stretches, so the stubs reach its edges.
        .flex_grow_1()
        .items_stretch()
        .gap(rems_from_px(PAD))
        // Side padding only: the stubs run to the top and the bottom. A clear border holds the
        // keyboard focus ring without moving the content.
        .px(rems_from_px(INSET - FOCUS))
        .border_2()
        .border_color(gpui::transparent_black())
        .rounded(rems_from_px(NODE_RADIUS));
    let row = match input.side {
        // Equal `flex_1` sides keep the tile on the row's middle, whatever the name's length.
        NodeSide::Middle => row
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .items_end()
                    .text_right()
                    .child(name_row(input, tone)),
            )
            .child(column)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .justify_center()
                    .items_start()
                    .child(state_chip(input, tone, accent, cx)),
            ),
        NodeSide::Left => row
            .child(
                texts(input, tone, accent, cx)
                    .items_end()
                    .text_right(),
            )
            .child(column),
        NodeSide::Right => row
            .child(column)
            .child(texts(input, tone, accent, cx).items_start()),
    };
    row.tooltip(Tooltip::text(tip))
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
        // A pair row stretches both nodes to one height, so their stubs end on one line.
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

#[path = "flow_details.rs"]
mod card;
pub(super) use card::details;
