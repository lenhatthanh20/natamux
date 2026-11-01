#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::rc::Rc;

use gpui::TestAppContext;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::CardInput;
use super::chips::pass_row;
use super::node::{INSET, chip_word, details, node, node_chip_word, tooltip_text};
use crate::app::test_views;
use crate::stages::card_rules::{CardButton, CardChip, CardFacts};
use crate::stages::flow_rules::CaretAt;
use crate::stages::stage_choices::{ChoiceBox, choice_box};
use crate::tasks::details_commands::{OnCommand, StageToggle};

const WIDTH: f32 = 240.;
const LONG: &str = "Sections Goal, Scope, Out of scope, Acceptance criteria and Code map";

/// One evidence row in a box of a fixed width.
struct Evidence {
    pass: &'static str,
}

impl Render for Evidence {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(px(WIDTH)).child(pass_row(self.pass))
    }
}

/// The bounds of the drawn row and of its text.
fn drawn(pass: &'static str, cx: &mut TestAppContext) -> [gpui::Bounds<Pixels>; 2] {
    let (_view, cx) = cx.add_window_view(|_, _| Evidence { pass });
    cx.run_until_parked();
    ["evidence-row", "evidence-text"].map(|selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} was not drawn"))
    })
}

#[gpui::test]
fn a_long_pass_wraps_inside_the_card(cx: &mut TestAppContext) {
    // A long pass kept its text width and ran past the card's edge (T-0190).
    test_views::init(cx);
    let [_, short] = drawn("spec.md was written", cx);
    let [row, long] = drawn(LONG, cx);
    assert!(row.right() <= px(WIDTH));
    assert!(long.right() <= row.right());
    assert!(long.size.height > short.size.height * 1.5);
}

/// The source of the function `name` in `flow_node.rs`, so the layout can be checked without a
/// window.
fn node_source(name: &str) -> &'static str {
    let source = include_str!("flow_node.rs");
    let start = source.find(name).unwrap_or_else(|| panic!("{name}"));
    let rest = &source[start..];
    let end = rest.find("\n}\n").expect("end of the function");
    &rest[..end]
}

#[test]
fn a_node_header_centres_the_tile_and_the_text() {
    // Beside a top-aligned head the mark sat above the stage name (T-0169).
    let source = node_source("fn header(");
    assert!(source.contains("items_center"));
    assert!(!source.contains("items_start"));
}

#[test]
fn a_node_root_does_not_clip_the_ports_of_the_flow() {
    // The flow draws its ports over the node's border; a clipped root would cut them.
    let source = node_source("pub(super) fn node(");
    assert!(!source.contains("overflow_hidden"));
    assert!(!source.contains("overflow_x_hidden"));
}

#[test]
fn a_node_has_no_more_button() {
    // The "..." menu took the name's room; every entry is a button or chip of the card (T-0252).
    let source = include_str!("flow_node.rs");
    assert!(!source.contains("IconName::Ellipsis"));
    assert!(!source.contains("ContextMenu"));
}

#[test]
fn a_node_header_has_even_padding() {
    // The head had 10 px left and right but 8 px top and bottom (T-0252).
    let source = node_source("fn header(");
    assert!(source.contains(".p(rems_from_px(INSET - FOCUS))"));
    assert!(!source.contains(".px("));
    assert!(!source.contains(".py("));
}

#[test]
fn a_node_is_selected_with_a_click_enter_and_space() {
    let source = node_source("fn header(");
    assert!(source.contains("\"enter\" | \"space\""));
    assert!(source.contains("PanelCommand::SelectStage"));
    // Every status selects, the current stage too: no early return for some nodes (T-0257).
    assert!(!source.contains("return head"));
}

#[test]
fn a_bare_node_has_no_border_and_no_background() {
    // Only the selected node gets a ring and a fill (T-0257).
    let source = node_source("pub(super) fn node(");
    assert!(!source.contains(".border_1()"));
    assert!(!source.contains("elevated_surface_background"));
    assert!(source.contains("when(input.selected"));
}

fn input(status: StageStatus, buttons: Vec<CardButton>) -> CardInput {
    let mut stage = Stage::new("plan", StageKind::Agent, Gate::Auto);
    stage.output = Some("plan.md".into());
    CardInput {
        compact: false,
        entry: StageProgress {
            stage: "plan".into(),
            name: "Plan".into(),
            kind: StageKind::Agent,
            gate: Gate::Auto,
            output: None,
            group: None,
            path: None,
            because: None,
            status,
            state: None,
            rounds: 1,
            started_at: None,
            ended_at: None,
            approved_by: None,
            passes: Vec::new(),
            reasons: Vec::new(),
            sent_back_for: None,
            sent_back_by_ci: false,
        },
        stage,
        now_ms: 0,
        selected: false,
        node_agent: Some(("claude-code".into(), true)),
        file: None,
        pr: None,
        usage: None,
        activity: None,
        ci_branch: None,
        ran_workflow: false,
        missing_skills: Vec::new(),
        skills_agent: String::new(),
        facts: CardFacts {
            chip: CardChip::NotStarted,
            buttons,
            notices: Vec::new(),
        },
        evidence_open: false,
        fold_focus: None,
        boxes: Vec::new(),
        skip: None,
        actions: None,
    }
}

#[test]
fn the_meta_line_and_the_tooltip_say_the_same_facts() {
    assert_eq!(chip_word(CardChip::Done), None);
    assert_eq!(chip_word(CardChip::WillBeSkipped), Some("Will be skipped"));
    assert_eq!(
        chip_word(CardChip::State(TaskState::Review)),
        Some("In review")
    );
    let facts = ["2m".to_owned(), "$0.61 est.".to_owned()];
    assert_eq!(
        tooltip_text("Plan", Some("Running"), &facts),
        "Plan\nRunning\n2m\n$0.61 est."
    );
    assert_eq!(tooltip_text("Plan", None, &[]), "Plan");
}

fn noop() -> OnCommand {
    Rc::new(|_, _, _| {})
}

/// One choice box in a box of a fixed width.
struct Choice {
    width: f32,
}

impl Render for Choice {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let choice = ChoiceBox {
            kind: StageToggle::Workflow,
            stage: "plan".into(),
            on: false,
            busy: false,
            allowed: true,
            // The hint goes into a tooltip, so the row is the label alone.
            started: true,
        };
        v_flex()
            .w(px(self.width))
            .child(choice_box(&choice, &noop(), cx))
    }
}

fn drawn_choice(width: f32, cx: &mut TestAppContext) -> [gpui::Bounds<Pixels>; 2] {
    let (_view, cx) = cx.add_window_view(|_, _| Choice { width });
    cx.run_until_parked();
    ["choice-row", "choice-label"].map(|selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} was not drawn"))
    })
}

#[gpui::test]
fn a_long_choice_wraps_inside_the_card(cx: &mut TestAppContext) {
    // "Run with a workflow" ran past the right edge of a narrow card (T-0252).
    const NARROW: f32 = 90.;
    test_views::init(cx);
    let [_, wide] = drawn_choice(400., cx);
    let [row, narrow] = drawn_choice(NARROW, cx);
    assert!(row.right() <= px(NARROW));
    assert!(narrow.right() <= row.right());
    assert!(narrow.size.height > wide.size.height * 1.5);
}

/// One node, or the details card of the selected node, in a box of the rail's width.
struct Node {
    status: StageStatus,
    details: bool,
}

const RAIL: f32 = 300.;

impl Render for Node {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut card = input(self.status, Vec::new());
        card.facts.chip = match self.status {
            StageStatus::Current => CardChip::State(TaskState::Running),
            _ => CardChip::Done,
        };
        card.usage = Some("$0.11".into());
        // A node of a pair: the details card still takes the whole width.
        card.compact = true;
        let drawn = if self.details {
            details(card, CaretAt::Middle, &noop(), cx)
        } else {
            node(card, &noop(), cx)
        };
        v_flex().w(px(RAIL)).child(drawn)
    }
}

fn drawn_node(
    status: StageStatus,
    details: bool,
    cx: &mut TestAppContext,
) -> &mut gpui::VisualTestContext {
    let (_view, cx) = cx.add_window_view(|_, _| Node { status, details });
    cx.run_until_parked();
    cx
}

fn bounds(cx: &mut gpui::VisualTestContext, selector: &'static str) -> gpui::Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

#[gpui::test]
fn a_bare_node_centres_its_tile_and_name_and_shows_no_facts(cx: &mut TestAppContext) {
    test_views::init(cx);
    let cx = drawn_node(StageStatus::Done, false, cx);
    let [card, tile, texts] = ["node-card", "node-tile", "node-texts"].map(|s| bounds(cx, s));
    let left = tile.left() - card.left();
    let right = card.right() - texts.right();
    assert!((left - right).abs() <= px(1.), "{left:?} vs {right:?}");
    // Time, cost and the state word are on the details card only (T-0257).
    assert!(cx.debug_bounds("node-meta").is_none());
}

#[gpui::test]
fn the_details_card_of_a_pair_node_takes_the_whole_width(cx: &mut TestAppContext) {
    test_views::init(cx);
    let cx = drawn_node(StageStatus::Current, true, cx);
    let card = bounds(cx, "node-details");
    assert!((card.size.width - px(RAIL)).abs() <= px(0.5), "{card:?}");
    // The name and facts head the card as in mockup A: no kind tile, the caret points at it.
    assert!(cx.debug_bounds("node-tile").is_none());
    let meta = bounds(cx, "node-meta");
    assert!(
        (meta.left() - card.left() - px(INSET + 1.)).abs() <= px(0.5),
        "{meta:?}"
    );
}

#[test]
fn the_state_chip_says_done_the_state_or_not_yet() {
    assert_eq!(node_chip_word(CardChip::Done), "Done");
    assert_eq!(node_chip_word(CardChip::NotStarted), "Not yet");
    assert_eq!(
        node_chip_word(CardChip::State(TaskState::Running)),
        "Running"
    );
    assert_eq!(node_chip_word(CardChip::WillBeSkipped), "Will be skipped");
}

#[test]
fn a_lit_tile_and_its_chip_glow_and_a_tile_not_run_is_dashed() {
    let tile = node_source("fn tile(");
    assert!(tile.contains(".shadow(glow("));
    assert!(tile.contains("border_dashed"));
    assert!(tile.contains("agent_badge(input, cx)"));
    assert!(node_source("fn state_chip(").contains(".shadow(glow("));
    // The name is not bold on the node.
    assert!(!node_source("fn name_label(").contains("SEMIBOLD"));
}

#[gpui::test]
fn the_agent_badge_sits_on_the_tiles_corner(cx: &mut TestAppContext) {
    test_views::init(cx);
    let cx = drawn_node(StageStatus::Done, false, cx);
    let [tile, badge, chip] = ["node-tile", "node-agent", "node-chip"].map(|s| bounds(cx, s));
    assert!(badge.right() > tile.right() && badge.bottom() > tile.bottom());
    assert!(badge.left() < tile.right() && badge.top() < tile.bottom());
    // The chip sits beside the tile, under the name.
    assert!(chip.left() > tile.right());
}
