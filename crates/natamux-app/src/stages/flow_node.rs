//! One stage as a node of the Progress flow (T-0257, T-0259): a bare node (the kind tile with the
//! state mark and the agent badge on its corners, the name under it) with no border or
//! background, and the full-width details card of the selected node, with a caret pointing up at
//! it. The tile and the name sit on the node's middle line. The node draws the line into its tile
//! and the line out from under its name, so the lines touch it; nothing here may clip (no
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
    INSET, LinkStyle, NodeTone, RUN_RING_OUT, STUB, Stroke, TILE, TILE_BOX, node_tone,
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
/// The gap between the tile and the name under it.
const NAME_GAP: f32 = 6.;
/// A long name wraps at this width, so it stays under the tile instead of spanning the panel.
pub(super) const NAME_MAX: f32 = 144.;
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
/// dark while it has not run, with the state mark on its top corner and the agent badge on its
/// bottom corner.
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
        .child(state_mark(input, accent, cx))
        .children(agent_badge(input, cx))
}

/// The state mark on the tile's top corner (owner's picture, T-0259): a filled check once done,
/// the state's glyph while current (it blinks while running), the CI mark while its CI runs, and
/// a dashed ring before it runs. The state's word is in the node's tooltip.
fn state_mark(input: &CardInput, accent: Hsla, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let mark = div()
        .debug_selector(|| "node-mark".into())
        .absolute()
        .right(rems_from_px(-BADGE_OUT))
        .top(rems_from_px(-BADGE_OUT))
        .size(rems_from_px(BADGE))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1();
    match (input.facts.chip, &input.ci_branch) {
        (CardChip::Done, _) => mark.border_color(accent).bg(accent).child(
            Icon::new(IconName::Check)
                .size(IconSize::XSmall)
                .color(Color::Custom(colors.panel_background)),
        ),
        (CardChip::State(_), Some(_)) => {
            mark.border_color(accent).bg(colors.panel_background).child(
                Icon::new(IconName::GitBranch)
                    .size(IconSize::XSmall)
                    .color(Color::Custom(accent)),
            )
        }
        (CardChip::State(state), None) => {
            mark.border_color(accent).bg(colors.panel_background).child(
                StateGlyph::new(
                    SharedString::from(format!("glyph-{}", input.entry.stage)),
                    Some(state),
                )
                .size(ShapeSize::Terminal),
            )
        }
        _ => mark
            .border_dashed()
            .border_color(colors.text_muted)
            .bg(colors.panel_background),
    }
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

/// The name of a bare node, centred under its tile. Wider than `NAME_MAX`, it wraps, so a name
/// that fits the panel still stays under the tile. The line out starts right under it.
fn name_row(input: &CardInput, tone: NodeTone, lift: f32) -> Div {
    div()
        .debug_selector(|| "node-name".into())
        .min_w_0()
        .max_w(rems_from_px(NAME_MAX))
        .pt(rems_from_px(NAME_GAP + lift))
        .text_center()
        .child(name_label(input, tone).text_center())
}

/// The line into the tile, `STUB` and `lift` high; the first step gets an empty spacer as high.
fn stub_in(style: Option<LinkStyle>, lift: f32) -> AnyElement {
    let high = STUB + lift;
    match style {
        Some(style) => FlowLink::stub(style, high).into_any_element(),
        None => div().flex_none().h(rems_from_px(high)).into_any_element(),
    }
}

/// The line out of the node, from under the name to the node's bottom. It grows, so the two
/// nodes of a pair, stretched to one height, end their lines on one row.
fn stub_out(style: Option<LinkStyle>) -> AnyElement {
    match style {
        Some(style) => FlowLink::stub(style, STUB).grow().into_any_element(),
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

/// The node, as the owner's picture (T-0259): the line in, the tile (in its ring while running),
/// the name under it, the line out, all on the node's middle line. A click, Enter and Space
/// select it; on the selected node they go back to the current stage.
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
    let tile = tile(input, tone, accent, cx);
    let running = matches!(
        tone,
        NodeTone::Current {
            state: TaskState::Running,
            ..
        }
    );
    // A tile with no ring is lifted by the ring's width above and below, so every tile and name
    // sits at the same height whatever the state (a pair of a running and a done stage).
    let (middle, lift) = if running {
        (RunningRing { accent, tile }.into_any_element(), 0.)
    } else {
        (tile.into_any_element(), RUN_RING_OUT)
    };
    let on_click = on.clone();
    let select = PanelCommand::SelectStage(entry.stage.clone());
    v_flex()
        .id(SharedString::from(format!("fold-{}", entry.stage)))
        .relative()
        .w_full()
        // Fills a node that a pair row stretches, so the line out reaches its bottom.
        .flex_grow_1()
        .items_center()
        // Side padding only and no border: the lines run to the top and the bottom.
        .px(rems_from_px(INSET))
        .rounded(rems_from_px(NODE_RADIUS))
        .child(stub_in(input.links.into, lift))
        .child(middle)
        .child(name_row(input, tone, lift))
        .child(stub_out(input.links.out))
        .child(focus_ring(input, on, colors.border_focused))
        .tooltip(Tooltip::text(tip))
        .cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .on_click(move |_, window, cx| on_click(select.clone(), window, cx))
}

/// The keyboard focus ring: absolute over the head, so it takes no room. It holds the node's
/// focus, so Enter and Space select the node, as a button of the web.
fn focus_ring(input: &CardInput, on: &OnCommand, color: Hsla) -> Stateful<Div> {
    let on_key = on.clone();
    let select = PanelCommand::SelectStage(input.entry.stage.clone());
    div()
        .id(SharedString::from(format!("focus-{}", input.entry.stage)))
        .debug_selector(|| "node-focus".into())
        .absolute()
        .inset_0()
        .rounded(rems_from_px(NODE_RADIUS))
        .border_2()
        .border_color(gpui::transparent_black())
        .focus_visible(move |style| style.border_color(color))
        .when_some(input.fold_focus.as_ref(), |this, focus| {
            this.track_focus(focus)
        })
        .on_key_down(move |event: &KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                cx.stop_propagation();
                on_key(select.clone(), window, cx);
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
