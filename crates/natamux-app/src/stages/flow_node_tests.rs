#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! A node on the flow's line, as the owner's picture draws it (T-0259): the tile in the middle,
//! the name under it, the lines touching the tile and the name, the running ring.

use gpui::{Bounds, TestAppContext, VisualTestContext};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::node::{NAME_GAP, NAME_MAX, node};
use super::tests::{input, noop};
use crate::app::test_views;
use crate::stages::card_rules::CardChip;
use crate::stages::flow_rules::{LinkStyle, NodeLinks, PAIR_GAP, RUN_RING_OUT, STUB, pair_tiles};

const RAIL: f32 = 300.;
const SHORT: &str = "Spec";
const PICTURE: &str = "Watch CI/CD and merge PRs";
const LONG: &str = "Watch CI/CD and merge the pull requests of every repository";

/// Where the node is drawn: alone in the rail, or in one half of a pair row.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Alone,
    Left,
    Right,
}

/// One node in the rail.
#[derive(Clone, Copy)]
struct Flow {
    status: StageStatus,
    name: &'static str,
    place: Place,
    links: NodeLinks,
    /// An upcoming stage marked to skip.
    marked: bool,
}

impl Render for Flow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut card = input(self.status, Vec::new());
        card.entry.name = self.name.into();
        card.links = self.links;
        card.facts.chip = match self.status {
            StageStatus::Current => {
                card.entry.state = Some(TaskState::Running);
                CardChip::State(TaskState::Running)
            }
            StageStatus::Skipped => CardChip::Skipped,
            StageStatus::Upcoming if self.marked => CardChip::WillBeSkipped,
            StageStatus::Upcoming => CardChip::NotStarted,
            _ => CardChip::Done,
        };
        let drawn = node(card, &noop(), cx);
        // As `progress_steps.rs` lays out a pair: two `flex_1` boxes `PAIR_GAP` apart.
        let slot = || v_flex().flex_1().min_w_0();
        let row = h_flex()
            .debug_selector(|| "flow-row".into())
            .w(px(RAIL))
            .items_stretch()
            .gap(rems_from_px(PAIR_GAP));
        let row = match self.place {
            Place::Alone => row.child(slot().child(drawn)),
            Place::Left => row.child(slot().child(drawn)).child(slot()),
            Place::Right => row.child(slot()).child(slot().child(drawn)),
        };
        // A block around the row, so the window does not stretch it.
        div().child(row)
    }
}

fn drawn(flow: Flow, cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (_view, cx) = cx.add_window_view(|_, _| flow);
    cx.run_until_parked();
    cx
}

fn alone(status: StageStatus, name: &'static str, links: NodeLinks) -> Flow {
    Flow {
        status,
        name,
        place: Place::Alone,
        links,
        marked: false,
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

const LINKS: NodeLinks = NodeLinks {
    into: Some(LinkStyle::Done),
    out: Some(LinkStyle::Done),
};

#[gpui::test]
fn the_tile_and_the_name_sit_on_the_middle_line(cx: &mut TestAppContext) {
    test_views::init(cx);
    for status in [StageStatus::Done, StageStatus::Current] {
        for name in ["P", SHORT, LONG] {
            let cx = drawn(alone(status, name, NodeLinks::default()), cx);
            let [row, tile, label] = ["flow-row", "node-tile", "node-name"].map(|s| bounds(cx, s));
            assert!(near(centre_x(tile), centre_x(row)), "{name}: {tile:?}");
            assert!(near(centre_x(label), centre_x(row)), "{name}: {label:?}");
            // The name is under the tile; its box holds the gap above the text.
            assert!(label.top() >= tile.bottom() - px(0.5), "{name}: {label:?}");
        }
    }
}

#[gpui::test]
fn the_name_sits_close_under_the_tile(cx: &mut TestAppContext) {
    // The name sat 12 px under the tile, too far to read as part of the node (T-0265). It still
    // clears the running ring and the agent badge, both `RUN_RING_OUT` below the tile.
    test_views::init(cx);
    for status in [StageStatus::Done, StageStatus::Current] {
        let cx = drawn(alone(status, SHORT, LINKS), cx);
        let [tile, label] = ["node-tile", "node-name"].map(|s| bounds(cx, s));
        let gap = label.top() - tile.bottom();
        assert!(near(gap, px(RUN_RING_OUT + NAME_GAP)), "{status:?}: {gap:?}");
        assert!(gap < px(12.) - px(0.5), "{status:?}: {gap:?}");
    }
}

#[gpui::test]
fn a_long_name_wraps_under_the_tile(cx: &mut TestAppContext) {
    test_views::init(cx);
    let short = bounds(
        drawn(alone(StageStatus::Done, SHORT, NodeLinks::default()), cx),
        "node-name",
    );
    // The rail is wider than one line of the picture's name, so only the cap wraps it.
    assert!(short.size.width < px(NAME_MAX), "{short:?}");
    for name in [PICTURE, LONG] {
        let cx = drawn(alone(StageStatus::Done, name, NodeLinks::default()), cx);
        let [row, tile, long] = ["flow-row", "node-tile", "node-name"].map(|s| bounds(cx, s));
        assert!(
            long.size.height > short.size.height * 1.5,
            "{name}: {long:?}"
        );
        assert!(
            long.size.width <= px(NAME_MAX) + px(0.5),
            "{name}: {long:?}"
        );
        assert!(
            long.size.width + px(1.) < row.size.width,
            "{name}: {long:?}"
        );
        assert!(long.left() >= row.left() && long.right() <= row.right());
        assert!(near(centre_x(long), centre_x(tile)), "{name}: {long:?}");
    }
}

#[gpui::test]
fn a_struck_name_strikes_only_its_words(cx: &mut TestAppContext) {
    // GPUI struck a wrapped, centred label from the box's left edge on the second row, so the
    // line ran past "merge PRs" (T-0263). The node paints one line per row over its words.
    test_views::init(cx);
    let source = include_str!("flow_node.rs");
    let row = &source[source.find("fn name_row(").unwrap()..];
    let row = &row[..row.find("\n}\n").unwrap()];
    assert!(row.contains("struck_name("));
    assert!(!row.contains("strikethrough"));
    for name in [SHORT, PICTURE] {
        let plain = bounds(
            drawn(alone(StageStatus::Done, name, NodeLinks::default()), cx),
            "node-name",
        );
        let skipped = Flow {
            status: StageStatus::Skipped,
            ..alone(StageStatus::Done, name, NodeLinks::default())
        };
        let marked = Flow {
            status: StageStatus::Upcoming,
            marked: true,
            ..skipped
        };
        for flow in [skipped, marked] {
            let cx = drawn(flow, cx);
            let [tile, struck] = ["node-tile", "node-name"].map(|s| bounds(cx, s));
            // Same rows, same box: the lines take no room.
            assert!(
                near(struck.size.height, plain.size.height),
                "{name}: {struck:?}"
            );
            assert!(
                near(struck.size.width, plain.size.width),
                "{name}: {struck:?}"
            );
            assert!(near(centre_x(struck), centre_x(tile)), "{struck:?}");
        }
    }
}

#[gpui::test]
fn every_tile_and_name_sit_at_one_height_whatever_the_state(cx: &mut TestAppContext) {
    // A pair of a running and a done stage keeps its tiles and names on one row.
    test_views::init(cx);
    let mut at = |status| {
        let cx = drawn(alone(status, SHORT, LINKS), cx);
        let [card, tile, label] = ["node-card", "node-tile", "node-name"].map(|s| bounds(cx, s));
        (tile.top() - card.top(), label.bottom() - card.top())
    };
    let done = at(StageStatus::Done);
    let running = at(StageStatus::Current);
    assert!(
        near(done.0, running.0) && near(done.1, running.1),
        "{done:?} {running:?}"
    );
}

#[gpui::test]
fn the_lines_touch_the_tile_and_start_under_the_name(cx: &mut TestAppContext) {
    test_views::init(cx);
    let cx = drawn(alone(StageStatus::Done, LONG, LINKS), cx);
    let [card, tile, label, focus] =
        ["node-card", "node-tile", "node-name", "node-focus"].map(|s| bounds(cx, s));
    // The line in runs from the node's top to the tile; the line out from the name to the bottom.
    assert!(
        near(tile.top() - card.top(), px(STUB + RUN_RING_OUT)),
        "{tile:?}"
    );
    assert!(
        near(card.bottom() - label.bottom(), px(STUB)),
        "{card:?} {label:?}"
    );
    // The focus ring lies over the node and takes no room.
    assert!(near(focus.top(), card.top()) && near(focus.bottom(), card.bottom()));
    let source = include_str!("flow_node.rs");
    let header = &source[source.find("fn header(").unwrap()..];
    let order = [
        "stub_in(input.links.into, lift)",
        ".child(middle)",
        "name_row(",
        "stub_out(",
    ]
    .map(|part| header.find(part).unwrap_or_else(|| panic!("{part}")));
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
}

#[gpui::test]
fn the_running_node_has_a_ring_around_its_tile(cx: &mut TestAppContext) {
    test_views::init(cx);
    let done = drawn(alone(StageStatus::Done, SHORT, NodeLinks::default()), cx);
    assert!(done.debug_bounds("node-run-ring").is_none());
    let cx = drawn(alone(StageStatus::Current, SHORT, LINKS), cx);
    let [card, ring, tile] = ["node-card", "node-run-ring", "node-tile"].map(|s| bounds(cx, s));
    let out = px(RUN_RING_OUT);
    assert!(near(ring.left(), tile.left() - out) && near(ring.right(), tile.right() + out));
    assert!(near(ring.top(), tile.top() - out) && near(ring.bottom(), tile.bottom() + out));
    // The line in touches the ring.
    assert!(near(ring.top() - card.top(), px(STUB)), "{ring:?}");
    // It pulses on the shared clock, still when the window has no focus.
    assert!(
        include_str!("flow_node.rs").contains("pulse(motion::frame(Moving::Pulse, window, cx))")
    );
}

#[gpui::test]
fn the_tiles_of_a_pair_sit_in_the_middle_of_each_node(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (left_x, right_x) = pair_tiles(RAIL, 1.);
    for name in ["Implement", PICTURE] {
        for (place, x) in [(Place::Left, left_x), (Place::Right, right_x)] {
            let flow = Flow {
                status: StageStatus::Done,
                name,
                place,
                links: NodeLinks::default(),
                marked: false,
            };
            let cx = drawn(flow, cx);
            let [row, tile, label] = ["flow-row", "node-tile", "node-name"].map(|s| bounds(cx, s));
            assert!(
                near(centre_x(tile) - row.left(), px(x)),
                "{name} {place:?}: {tile:?}"
            );
            assert!(
                near(centre_x(label) - row.left(), px(x)),
                "{name} {place:?}: {label:?}"
            );
        }
    }
}

#[test]
fn the_lines_have_no_ports() {
    let lines = include_str!("flow_lines.rs");
    assert!(!lines.contains("PORT_OUT"));
    assert!(!lines.contains("fn at_top"));
    assert!(!lines.contains("reach"));
    assert!(!include_str!("flow_rules.rs").contains("pub const PORT"));
}
