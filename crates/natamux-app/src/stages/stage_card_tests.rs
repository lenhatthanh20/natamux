#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use natamux_proto::stages::actions::StageFile;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, Stage, StageKind};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::chips::pass_row;
use super::node::{chip_word, menu_commands, tooltip_text};
use super::{CardInput, FileChip};
use crate::app::test_views;
use crate::stages::card_rules::{CardButton, CardChip, CardFacts};

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
fn a_foldable_node_opens_with_enter_and_space() {
    let source = node_source("fn header(");
    assert!(source.contains("\"enter\" | \"space\""));
    assert!(source.contains("PanelCommand::FoldCard"));
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
        opened: false,
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

fn labels(input: &CardInput) -> Vec<&'static str> {
    menu_commands(input)
        .0
        .iter()
        .map(|(label, _)| *label)
        .collect()
}

#[test]
fn the_menu_offers_what_the_buttons_allow() {
    let mut current = input(
        StageStatus::Current,
        vec![
            CardButton::PickAgent,
            CardButton::Skip,
            CardButton::SendBack,
        ],
    );
    assert_eq!(labels(&current), ["Skip", "Send back"]);
    assert!(menu_commands(&current).1);
    // The output is offered once it is written.
    current.file = Some(FileChip::Missing);
    assert_eq!(labels(&current), ["Skip", "Send back"]);
    current.file = Some(FileChip::Written(StageFile {
        name: "plan.md".into(),
        size: 10,
        modified_at: "2026-10-31T00:00:00Z".into(),
    }));
    assert_eq!(labels(&current), ["Open output", "Skip", "Send back"]);
}

#[test]
fn the_menu_of_an_upcoming_node_toggles_skip() {
    let mut upcoming = input(StageStatus::Upcoming, Vec::new());
    assert!(labels(&upcoming).is_empty());
    upcoming.skip = Some((false, false));
    assert_eq!(labels(&upcoming), ["Skip"]);
    upcoming.skip = Some((true, false));
    assert_eq!(labels(&upcoming), ["Don't skip"]);
    assert!(!menu_commands(&upcoming).1);
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
