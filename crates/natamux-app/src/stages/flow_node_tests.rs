#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A node on the flow's line, as the owner's picture draws it (T-0259): the tile on the line, the
//! texts around it, the lines touching it, the running ring.

use gpui::{Bounds, TestAppContext, VisualTestContext};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::node::node;
use super::tests::{input, noop};
use crate::app::test_views;
use crate::stages::card_rules::CardChip;
use crate::stages::flow_rules::{
    LinkStyle, NodeLinks, NodeSide, PAIR_GAP, RUN_RING_OUT, STUB, pair_tiles,
};

const RAIL: f32 = 300.;
const SHORT: &str = "Spec";
const LONG: &str = "Watch CI/CD and merge the pull requests of every repository";
/// `flow_node.rs`: the gap between the tile column and the texts, and the head's clear border.
const PAD: f32 = 12.;
const FOCUS: f32 = 2.;

/// One node in the rail, alone (`Middle`) or in its half of a pair row.
#[derive(Clone, Copy)]
struct Flow {
    status: StageStatus,
    name: &'static str,
    side: NodeSide,
    links: NodeLinks,
}

impl Render for Flow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut card = input(self.status, Vec::new());
        card.entry.name = self.name.into();
        card.side = self.side;
        card.links = self.links;
        card.facts.chip = if self.status == StageStatus::Current {
            card.entry.state = Some(TaskState::Running);
            CardChip::State(TaskState::Running)
        } else {
            CardChip::Done
        };
        let drawn = node(card, &noop(), cx);
        // As `progress_steps.rs` lays out a pair: two `flex_1` boxes `PAIR_GAP` apart.
        let slot = || v_flex().flex_1().min_w_0();
        let row = h_flex()
            .debug_selector(|| "flow-row".into())
            .w(px(RAIL))
            .items_stretch()
            .gap(rems_from_px(PAIR_GAP));
        match self.side {
            NodeSide::Middle => row.child(slot().child(drawn)),
            NodeSide::Left => row.child(slot().child(drawn)).child(slot()),
            NodeSide::Right => row.child(slot()).child(slot().child(drawn)),
        }
    }
}

fn drawn(flow: Flow, cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (_view, cx) = cx.add_window_view(|_, _| flow);
    cx.run_until_parked();
    cx
}

fn middle(status: StageStatus, name: &'static str, links: NodeLinks) -> Flow {
    Flow {
        status,
        name,
        side: NodeSide::Middle,
        links,
    }
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

fn centre_x(of: Bounds<Pixels>) -> Pixels {
    of.left() + of.size.width / 2.
}

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(0.5)
}

#[gpui::test]
fn a_tile_sits_on_the_middle_line_whatever_the_name(cx: &mut TestAppContext) {
    test_views::init(cx);
    // A running node's ring box keeps its tile on the same line.
    for status in [StageStatus::Done, StageStatus::Current] {
        for name in ["P", SHORT, LONG] {
            let cx = drawn(middle(status, name, NodeLinks::default()), cx);
            let [row, tile] = ["flow-row", "node-tile"].map(|s| bounds(cx, s));
            assert!(
                near(centre_x(tile), centre_x(row)),
                "{name}: {tile:?} {row:?}"
            );
        }
    }
}

#[gpui::test]
fn the_name_is_at_the_tiles_left_and_the_chip_at_its_right(cx: &mut TestAppContext) {
    test_views::init(cx);
    let cx = drawn(middle(StageStatus::Done, SHORT, NodeLinks::default()), cx);
    let [column, name, chip] =
        ["node-tile-column", "node-name", "node-chip"].map(|s| bounds(cx, s));
    assert!(near(name.right(), column.left() - px(PAD)), "{name:?}");
    assert!(near(chip.left(), column.right() + px(PAD)), "{chip:?}");
}

#[gpui::test]
fn a_long_name_wraps_beside_the_tile(cx: &mut TestAppContext) {
    test_views::init(cx);
    let short = bounds(
        drawn(middle(StageStatus::Done, SHORT, NodeLinks::default()), cx),
        "node-name",
    );
    let cx = drawn(middle(StageStatus::Done, LONG, NodeLinks::default()), cx);
    let [column, long] = ["node-tile-column", "node-name"].map(|s| bounds(cx, s));
    assert!(long.size.height > short.size.height * 1.5, "{long:?}");
    assert!(long.right() <= column.left() - px(PAD) + px(0.5));
}

#[gpui::test]
fn the_stubs_run_from_the_nodes_edges_to_its_tile(cx: &mut TestAppContext) {
    test_views::init(cx);
    let links = NodeLinks {
        into: Some(LinkStyle::Done),
        out: Some(LinkStyle::Done),
    };
    // A wrapped name makes the node taller than its tile: the stubs grow with it.
    for name in [SHORT, LONG] {
        let cx = drawn(middle(StageStatus::Done, name, links), cx);
        let [card, column, tile] =
            ["node-card", "node-tile-column", "node-tile"].map(|s| bounds(cx, s));
        // The stubs reach over the clear border, so they meet the flow's lines.
        assert!(
            near(column.top(), card.top() + px(FOCUS)),
            "{name}: {column:?}"
        );
        assert!(
            near(column.bottom(), card.bottom() - px(FOCUS)),
            "{name}: {column:?}"
        );
        assert!(tile.top() - column.top() >= px(STUB - 0.5));
        assert!(column.bottom() - tile.bottom() >= px(STUB - 0.5));
    }
    let source = include_str!("flow_node.rs");
    assert!(source.contains("stub(input.links.into, FOCUS, 0.)"));
    assert!(source.contains("stub(input.links.out, 0., FOCUS)"));
}

#[gpui::test]
fn the_running_node_has_a_ring_around_its_tile(cx: &mut TestAppContext) {
    test_views::init(cx);
    let done = drawn(middle(StageStatus::Done, SHORT, NodeLinks::default()), cx);
    assert!(done.debug_bounds("node-run-ring").is_none());
    let cx = drawn(
        middle(StageStatus::Current, SHORT, NodeLinks::default()),
        cx,
    );
    let [ring, tile] = ["node-run-ring", "node-tile"].map(|s| bounds(cx, s));
    let out = px(RUN_RING_OUT);
    assert!(near(ring.left(), tile.left() - out) && near(ring.right(), tile.right() + out));
    assert!(near(ring.top(), tile.top() - out) && near(ring.bottom(), tile.bottom() + out));
    // It pulses on the shared clock, still when the window has no focus.
    assert!(
        include_str!("flow_node.rs").contains("pulse(motion::frame(Moving::Pulse, window, cx))")
    );
}

#[gpui::test]
fn the_tiles_of_a_pair_sit_near_the_middle_with_the_texts_outside(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (left_x, right_x) = pair_tiles(RAIL, 1.);
    for (side, x) in [(NodeSide::Left, left_x), (NodeSide::Right, right_x)] {
        let flow = Flow {
            status: StageStatus::Done,
            name: "Implement",
            side,
            links: NodeLinks::default(),
        };
        let cx = drawn(flow, cx);
        let [row, column, tile, texts] =
            ["flow-row", "node-tile-column", "node-tile", "node-texts"].map(|s| bounds(cx, s));
        assert!(
            near(centre_x(tile) - row.left(), px(x)),
            "{side:?}: {tile:?}"
        );
        if side == NodeSide::Left {
            assert!(texts.right() <= column.left() - px(PAD) + px(0.5));
        } else {
            assert!(texts.left() >= column.right() + px(PAD) - px(0.5));
        }
    }
}

#[test]
fn the_lines_have_no_ports() {
    let lines = include_str!("flow_lines.rs");
    assert!(!lines.contains("PORT_OUT"));
    assert!(!lines.contains("fn at_top"));
    assert!(!include_str!("flow_rules.rs").contains("pub const PORT"));
}
