//! One stage as a node of the Progress flow (design E): a kind tile with the state glyph at its
//! corner, the name with its marks, a meta line, and the "..." menu. The body of a finished or
//! upcoming node opens and folds; the current node is always open. The node draws no ports or
//! lines: the flow draws them on its edges, so nothing here may clip (no `overflow_hidden` on
//! the root) and the glow ring is absolute, so it takes no room. Only the running glyph moves.

use gpui::{AnyElement, ClickEvent, Div, FontWeight, Hsla, KeyDownEvent, Pixels, Point, Stateful};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use ui::{ContextMenu, Tooltip, prelude::*};

use super::{CardInput, FileChip, badge, body, fact_parts, upcoming_parts};
use crate::agents::agent_choices::{AGENT_CHOICES, agent_label};
use crate::components::agent_badge::AgentIcon;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::menu::MenuLayer;
use crate::components::shape::ShapeSize;
use crate::stages::card_rules::CardChip;
use crate::stages::editor_labels::{StageRunner, model_of};
use crate::stages::flow_rules::{NodeTone, Stroke, node_tone};
use crate::stages::kind_icon::{kind_icon, waits_icon};
use crate::stages::preview_rules::agent_line_text;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::task_rules::state_label;

const TILE: f32 = 32.;
/// The tile of a node in a side-by-side pair, so its name has room.
const TILE_COMPACT: f32 = 24.;
const TILE_RADIUS: f32 = 8.;
const NODE_RADIUS: f32 = 9.;
/// Tile, then the gap and the node's padding: where the text starts.
const PAD: f32 = 10.;
/// The glow ring is 3 px wide, outside the border.
const RING: f32 = 3.;

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

/// The entries of the "..." menu, in order, and whether the agents to pick from follow them.
pub(super) fn menu_commands(input: &CardInput) -> (Vec<(&'static str, PanelCommand)>, bool) {
    use crate::stages::card_rules::CardButton;
    use crate::tasks::details_commands::StageToggle;
    let stage = &input.entry.stage;
    let mut items = Vec::new();
    if let (Some(output), Some(FileChip::Written(_))) = (&input.stage.output, &input.file) {
        items.push(("Open output", PanelCommand::OpenStageFile(output.clone())));
    }
    if let Some((marked, _)) = input.skip {
        let label = if marked { "Don't skip" } else { "Skip" };
        items.push((
            label,
            PanelCommand::Toggle {
                kind: StageToggle::Skip,
                stage: stage.clone(),
                on: !marked,
            },
        ));
    }
    let buttons = &input.facts.buttons;
    if buttons.contains(&CardButton::Skip) {
        items.push(("Skip", PanelCommand::Skip(stage.clone())));
    }
    if buttons.contains(&CardButton::SendBack) {
        items.push(("Send back", PanelCommand::SendBack(stage.clone())));
    }
    (items, buttons.contains(&CardButton::PickAgent))
}

fn open_more_menu(
    items: Vec<(&'static str, PanelCommand)>,
    pick_agent: bool,
    on: OnCommand,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        let has_items = !items.is_empty();
        let on_items = on.clone();
        let menu = items.into_iter().fold(menu, |menu, (label, command)| {
            let on = on_items.clone();
            menu.entry(label, None, move |window, cx| {
                on(command.clone(), window, cx)
            })
        });
        if !pick_agent {
            return menu;
        }
        let menu = if has_items { menu.separator() } else { menu };
        AGENT_CHOICES
            .iter()
            .fold(menu.header("Pick agent"), |menu, choice| {
                let on = on.clone();
                let id = choice.id.to_owned();
                menu.entry(choice.label, None, move |window, cx| {
                    on(PanelCommand::PickAgent(id.clone()), window, cx)
                })
            })
    });
    MenuLayer::open(menu, position, window, cx);
}

/// The agent of a stage with an agent of its own, with its model when the args name one.
/// A compact node shows the icon alone.
fn agent_chip(stage: &Stage, compact: bool) -> Option<AnyElement> {
    let agent = stage.agent.clone()?;
    let runner = StageRunner {
        agent: Some(agent),
        model: model_of(stage.args.as_deref()),
    };
    Some(
        // Shrinks before the name: in a narrow node the label goes first, the icon stays.
        h_flex()
            .min_w_0()
            .flex_shrink_1()
            .overflow_hidden()
            .gap_1()
            .child(
                div()
                    .flex_none()
                    .child(AgentIcon::new(runner.agent.clone())),
            )
            .when(!compact, |this| {
                this.child(
                    Label::new(agent_line_text(&runner, |agent| agent_label(Some(agent))))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .truncate(),
                )
            })
            .into_any_element(),
    )
}

/// The state glyph on the tile's corner; a node that checks on CI shows the CI mark instead.
fn overlay(input: &CardInput, tone: NodeTone, accent: Hsla, cx: &App) -> Option<AnyElement> {
    let state = tone.glyph_state()?;
    let colors = cx.theme().colors();
    let mark = if input.ci_branch.is_some() {
        Icon::new(IconName::GitBranch)
            .size(IconSize::XSmall)
            .color(Color::Custom(accent))
            .into_any_element()
    } else {
        StateGlyph::new(
            SharedString::from(format!("glyph-{}", input.entry.stage)),
            Some(state),
        )
        .size(ShapeSize::Terminal)
        .into_any_element()
    };
    Some(
        div()
            .absolute()
            .right(rems_from_px(-5.0_f32))
            .bottom(rems_from_px(-5.0_f32))
            .size(rems_from_px(14.0_f32))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.elevated_surface_background)
            .child(mark)
            .into_any_element(),
    )
}

fn tile(input: &CardInput, tone: NodeTone, accent: Hsla, cx: &App) -> Div {
    let colors = cx.theme().colors();
    let muted = tone.muted();
    div()
        .relative()
        .flex_none()
        .size(rems_from_px(if input.compact {
            TILE_COMPACT
        } else {
            TILE
        }))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rems_from_px(TILE_RADIUS))
        .border_1()
        .border_color(if muted {
            colors.border_variant
        } else {
            accent.opacity(0.42)
        })
        .when(!muted, |this| this.bg(accent.opacity(0.14)))
        .when(matches!(tone.border(), Stroke::Dashed), |this| {
            this.border_dashed()
        })
        .child(
            kind_icon(input.stage.kind)
                .size(IconSize::Small)
                .color(Color::Custom(accent)),
        )
        .children(overlay(input, tone, accent, cx))
}

/// The name with its marks: agent, the person mark of a manual gate, rounds and the workflow.
/// The row clips instead of wrapping, so a narrow node keeps its height.
fn name_row(input: &CardInput, tone: NodeTone, cx: &App) -> Div {
    let entry = &input.entry;
    let manual = input.stage.gate == Gate::Manual && input.stage.kind != StageKind::Release;
    let name = Label::new(entry.name.clone())
        .weight(FontWeight::SEMIBOLD)
        .when(input.compact, |label| label.size(LabelSize::Small))
        .color(if tone.muted() {
            Color::Muted
        } else {
            Color::Default
        })
        .when(tone.struck(), |label| label.strikethrough())
        .truncate();
    h_flex()
        .min_w_0()
        .gap_1p5()
        .overflow_hidden()
        // The name keeps its width up to 70%, so the agent chip gives way first.
        .child(
            div()
                .min_w_0()
                .flex_shrink_0()
                .max_w(relative(if input.compact { 1. } else { 0.7 }))
                .child(name),
        )
        .children(agent_chip(&input.stage, input.compact))
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

/// The "..." button: always there on the current node, on hover for the others.
fn more_button(
    input: &CardInput,
    current: bool,
    group: SharedString,
    on: &OnCommand,
) -> Option<IconButton> {
    let (items, pick_agent) = menu_commands(input);
    if items.is_empty() && !pick_agent {
        return None;
    }
    let on = on.clone();
    Some(
        IconButton::new(
            SharedString::from(format!("more-{}", input.entry.stage)),
            IconName::Ellipsis,
        )
        .icon_size(IconSize::Small)
        .tooltip(Tooltip::text("More"))
        .when(!current, |button| button.visible_on_hover(group))
        .on_click(move |event: &ClickEvent, window, cx| {
            // The header folds on a click; this one only opens the menu.
            cx.stop_propagation();
            open_more_menu(
                items.clone(),
                pick_agent,
                on.clone(),
                event.position(),
                window,
                cx,
            );
        }),
    )
}

/// The header row: tile, text, "..." . A finished or upcoming node folds on a click, Enter and
/// Space, as the finished card did.
fn header(
    input: &CardInput,
    tone: NodeTone,
    accent: Hsla,
    foldable: bool,
    on: &OnCommand,
    cx: &App,
) -> Stateful<Div> {
    let colors = cx.theme().colors();
    let entry = &input.entry;
    let current = entry.status == StageStatus::Current;
    let group = SharedString::from(format!("node-{}", entry.stage));
    let word = chip_word(input.facts.chip);
    let facts = fact_parts(input);
    let tip = tooltip_text(&entry.name, word, &facts);
    let hover_bg = colors.ghost_element_hover;
    let ring = colors.border_focused;
    let texts = v_flex()
        .flex_1()
        .min_w_0()
        .gap(rems_from_px(1.0_f32))
        .child(name_row(input, tone, cx))
        .children(meta_line(word, &facts, tone, accent));
    let head = h_flex()
        .id(SharedString::from(format!("fold-{}", entry.stage)))
        .group(group.clone())
        .w_full()
        .items_center()
        .gap(rems_from_px(PAD))
        // A clear 2 px border holds the keyboard focus ring without moving the content.
        .px(rems_from_px(PAD - 2.))
        .py(rems_from_px(6.0_f32))
        .border_2()
        .border_color(gpui::transparent_black())
        .rounded(rems_from_px(NODE_RADIUS))
        .child(tile(input, tone, accent, cx))
        .child(texts)
        .children(more_button(input, current, group, on))
        .tooltip(Tooltip::text(tip));
    if !foldable {
        return head;
    }
    let on_fold = on.clone();
    let fold = PanelCommand::FoldCard(entry.stage.clone());
    let (on_key, fold_key) = (on_fold.clone(), fold.clone());
    head.cursor_pointer()
        .hover(move |style| style.bg(hover_bg))
        .focus_visible(move |style| style.border_color(ring))
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
}

/// The still ring of the current node: absolute and outside the border, so it takes no room.
fn glow_ring(accent: Hsla) -> Div {
    let reach = rems_from_px(-(RING + 1.0));
    div()
        .absolute()
        .top(reach)
        .left(reach)
        .right(reach)
        .bottom(reach)
        .rounded(rems_from_px(NODE_RADIUS + RING))
        .border_3()
        .border_color(accent.opacity(0.18))
}

/// One node. Width is the parent's. `opened` of the input folds a finished or upcoming node.
pub(super) fn node(input: CardInput, on: &OnCommand, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let entry = &input.entry;
    let marked = input.facts.chip == CardChip::WillBeSkipped;
    let tone = node_tone(entry.status, entry.state, marked, input.ci_branch.is_some());
    let accent = tone
        .color_state()
        .map_or(colors.text_muted, |state| state_colors(state, cx).fg);
    let current = entry.status == StageStatus::Current;
    let foldable = matches!(
        entry.status,
        StageStatus::Done | StageStatus::Skipped | StageStatus::Upcoming
    );
    let open = current || (foldable && input.opened);
    let parts = match (open, entry.status) {
        (false, _) => Vec::new(),
        (true, StageStatus::Upcoming) => upcoming_parts(&input, on, cx),
        (true, _) => body(&input, on, cx),
    };
    let border = match tone {
        NodeTone::Current { .. } => accent.opacity(0.7),
        NodeTone::Upcoming | NodeTone::WillSkip => colors.border,
        _ => colors.border_variant,
    };
    let fade = match tone {
        NodeTone::Skipped => 0.7,
        NodeTone::NotTaken => 0.55,
        _ => 1.,
    };
    let head = header(&input, tone, accent, foldable, on, cx);
    // gpui has no dotted border: a not-taken node is dashed and faint.
    v_flex()
        .id(SharedString::from(format!("card-{}", entry.stage)))
        .relative()
        .w_full()
        .min_w_0()
        // A pair row stretches both nodes to one height, so their bottom ports line up.
        .flex_grow_1()
        .rounded(rems_from_px(NODE_RADIUS))
        .border_1()
        .border_color(border)
        .when(!tone.muted(), |this| {
            this.bg(colors.elevated_surface_background)
        })
        .when(!matches!(tone.border(), Stroke::Solid), |this| {
            this.border_dashed()
        })
        .when(fade < 1., |this| this.opacity(fade))
        .when(tone.glow(), |this| this.child(glow_ring(accent)))
        .child(head)
        .when(!parts.is_empty(), |this| {
            this.child(
                v_flex()
                    .gap_2()
                    .px(rems_from_px(PAD))
                    .pb(rems_from_px(PAD))
                    .children(parts),
            )
        })
        .into_any_element()
}
