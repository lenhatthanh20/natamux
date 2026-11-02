#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use gpui::{Bounds, TestAppContext, VisualTestContext};
use natamux_proto::events::task_event::WaitReason;
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{
    BranchPath, Condition, Gate, Stage, StageBranch, StageKind, Step,
};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::{ProgressInput, timeline};
use crate::app::test_views;
use crate::tasks::details_commands::OnCommand;

struct BranchView {
    input: ProgressInput,
    width: f32,
}

impl Render for BranchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on: OnCommand = Rc::new(|_, _, _| {});
        v_flex()
            .w(px(self.width))
            .children(timeline(&self.input, &on, cx))
    }
}

fn progress(id: &str, status: StageStatus) -> StageProgress {
    StageProgress {
        stage: id.into(),
        name: id.into(),
        kind: StageKind::Agent,
        gate: Gate::Auto,
        output: None,
        group: None,
        path: Some("first".into()),
        because: None,
        status,
        state: (status == StageStatus::Current).then_some(TaskState::Running),
        rounds: 1,
        started_at: None,
        ended_at: None,
        approved_by: None,
        passes: Vec::new(),
        reasons: Vec::new(),
        sent_back_for: None,
        sent_back_by_ci: false,
    }
}

fn branch_steps(taken: bool) -> Vec<Step> {
    let stage = |id| Stage::new(id, StageKind::Agent, Gate::Auto);
    let first = BranchPath {
        id: "first".into(),
        name: Some("First".into()),
        when: Some(Condition::Command { run: "true".into() }),
        not: None,
        stages: ["one", "two", "three"].into_iter().map(stage).collect(),
    };
    let other = BranchPath {
        id: "other".into(),
        name: None,
        when: Some(Condition::Command { run: "true".into() }),
        not: None,
        stages: vec![stage("other")],
    };
    let paths = if taken {
        vec![first, other]
    } else {
        vec![other, first]
    };
    vec![Step::Branch(StageBranch {
        id: "branch".into(),
        name: Some("Choice".into()),
        paths,
    })]
}

fn branch_input(taken: bool, expanded: bool) -> ProgressInput {
    let first_status = if taken {
        StageStatus::Done
    } else {
        StageStatus::NotTaken
    };
    let mut progress = vec![
        progress("one", first_status),
        progress(
            "two",
            if taken {
                StageStatus::Current
            } else {
                StageStatus::NotTaken
            },
        ),
        progress(
            "three",
            if taken {
                StageStatus::Upcoming
            } else {
                StageStatus::NotTaken
            },
        ),
        progress(
            "other",
            if taken {
                StageStatus::NotTaken
            } else {
                StageStatus::Current
            },
        ),
    ];
    progress[3].path = Some("other".into());
    ProgressInput {
        task: Task {
            state: TaskState::Running,
            ..Task::default()
        },
        steps: branch_steps(taken),
        progress,
        events: Vec::new(),
        files: None,
        usage: None,
        activity: None,
        wait: WaitReason::Stopped,
        now_ms: 0,
        busy: false,
        resuming: false,
        toggling: BTreeSet::new(),
        selected: Some("two".into()),
        task_agent: "claude-code".into(),
        open_lanes: if expanded {
            BTreeSet::from(["branch/first".into()])
        } else {
            BTreeSet::new()
        },
        open_evidence: BTreeSet::new(),
        card_focus: BTreeMap::new(),
        agent_label: String::new(),
        can_answer: false,
        can_act: false,
    }
}

fn drawn(input: ProgressInput, width: f32, cx: &mut TestAppContext) -> &mut VisualTestContext {
    let (_view, cx) = cx.add_window_view(|_, _| BranchView { input, width });
    cx.run_until_parked();
    cx
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"))
}

#[gpui::test]
fn selected_branch_node_opens_its_card_before_the_next_node(cx: &mut TestAppContext) {
    test_views::init(cx);
    for width in [320., 560.] {
        for taken in [true, false] {
            let view = drawn(branch_input(taken, !taken), width, cx);
            let selected = bounds(view, "selected-node-ring");
            let details = bounds(view, "node-details");
            let next = bounds(view, "node-card");
            let gap = details.top() - selected.bottom();
            assert!(gap >= px(0.) && gap < px(40.), "{width} {taken}: {gap:?}");
            assert!(
                details.bottom() <= next.top(),
                "{width} {taken}: {details:?} {next:?}"
            );
            assert!(details.right() <= px(width), "{width} {taken}: {details:?}");
        }
    }
}

#[gpui::test]
fn a_folded_branch_path_hides_its_selected_node_and_card(cx: &mut TestAppContext) {
    test_views::init(cx);
    let cx = drawn(branch_input(false, false), 320., cx);
    assert!(cx.debug_bounds("node-details").is_none());
}

/// The source of the function `name` in `file`, so the order of the flow's parts can be checked
/// without a window.
fn source_of(file: &'static str, name: &str) -> &'static str {
    let start = file.find(name).unwrap_or_else(|| panic!("{name}"));
    let rest = &file[start..];
    let end = rest.find("\n    }\n").expect("end of the function");
    &rest[..end]
}

/// `parts` come in this order in `source`.
fn in_order(source: &str, parts: &[&str]) {
    let at: Vec<usize> = parts
        .iter()
        .map(|part| source.find(part).unwrap_or_else(|| panic!("{part}")))
        .collect();
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{parts:?}");
}

// Mockup A (T-0257): the card sits right under the selected node's row with a caret, no
// connector between them, and the line goes on from the card's bottom. Owner, after the first
// build drew a connector and a port between the node and the card.

#[test]
fn a_stage_puts_its_card_right_under_the_node_with_no_port_between() {
    let step = source_of(include_str!("progress_flow.rs"), "fn flow_step(");
    in_order(
        step,
        &[
            "ported(card)",
            ".children(details)",
            "out.map(FlowLink::new)",
        ],
    );
}

#[test]
fn a_pair_puts_its_card_between_the_nodes_and_the_join_curves() {
    let pair = source_of(include_str!("progress_steps.rs"), "fn pair(");
    assert!(pair.contains("CaretAt::Left") && pair.contains("CaretAt::Right"));
    in_order(
        pair,
        &[".child(right_node)", ".children(details)", ".child(join)"],
    );
}

#[test]
fn a_stack_puts_its_card_under_the_whole_frame() {
    let stack = source_of(include_str!("progress_steps.rs"), "fn stack(");
    in_order(
        stack,
        &[".child(frame)", ".children(details)", "FlowLink::new(out)"],
    );
}

#[test]
fn a_node_of_a_path_places_its_card_inside_the_branch() {
    let step = source_of(include_str!("progress_flow.rs"), "fn flow_step(");
    in_order(
        step,
        &[
            "ported(card)",
            ".children(details)",
            "out.map(FlowLink::new)",
        ],
    );
    let lane = source_of(include_str!("progress_branch.rs"), "fn lane(");
    assert!(lane.contains("self.column(&steps, cx)"));
    let branch = source_of(include_str!("progress_branch.rs"), "pub(super) fn branch(");
    assert!(!branch.contains(".children(details)"));
}

#[test]
fn the_lines_meet_the_nodes_with_no_port_rings() {
    // The owner's node design: the lines run straight into the nodes, no ring at their ends.
    for source in [
        include_str!("progress_flow.rs"),
        include_str!("progress_steps.rs"),
        include_str!("progress_branch.rs"),
    ] {
        assert!(!source.contains("Port::"));
    }
}

#[test]
fn a_pair_says_side_by_side_and_waits_for_both() {
    // The owner's picture (T-0259).
    let pair = source_of(include_str!("progress_steps.rs"), "fn pair(");
    assert!(pair.contains("side(first)") && pair.contains("side(second)"));
    assert_eq!(super::steps::SIDE_BY_SIDE, "SIDE BY SIDE");
    let note = source_of(include_str!("progress_steps.rs"), "fn group_note(");
    assert!(note.contains("\"{name} waits for both\""));
}

#[test]
fn a_stage_draws_its_stubs_but_none_under_an_open_card() {
    let step = source_of(include_str!("progress_flow.rs"), "fn flow_step(");
    assert!(step.contains("into: around.in_style(own)"));
    assert!(step.contains("out: out.filter(|_| !open)"));
    let pair = source_of(include_str!("progress_steps.rs"), "fn pair(");
    assert!(pair.contains("out: (!open).then(|| join_style(edge.status))"));
}
