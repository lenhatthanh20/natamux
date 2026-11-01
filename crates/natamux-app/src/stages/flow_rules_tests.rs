use natamux_proto::stages::progress::StageStatus::{Current, Done, NotTaken, Skipped, Upcoming};
use natamux_proto::stages::schema::{Condition, Gate, Stage, StageKind};

use super::*;

#[test]
fn styles_a_connector_by_the_stage_it_leads_into() {
    assert_eq!(link_style(Done, Done, None), LinkStyle::Done);
    assert_eq!(
        link_style(Done, Current, Some(TaskState::NeedsInput)),
        LinkStyle::Current(TaskState::NeedsInput)
    );
    assert_eq!(
        link_style(Skipped, Current, None),
        LinkStyle::Current(TaskState::Ready)
    );
    assert_eq!(link_style(Current, Upcoming, None), LinkStyle::Upcoming);
    assert_eq!(link_style(Done, Skipped, None), LinkStyle::Skipped);
    assert_eq!(link_style(Done, NotTaken, None), LinkStyle::NotTaken);
    assert_eq!(link_style(NotTaken, Done, None), LinkStyle::NotTaken);
}

#[test]
fn draws_done_and_current_solid_upcoming_dashed_and_not_taken_dotted() {
    assert_eq!(LinkStyle::Done.stroke(), Stroke::Solid);
    assert_eq!(
        LinkStyle::Current(TaskState::Running).stroke(),
        Stroke::Solid
    );
    assert_eq!(LinkStyle::Upcoming.stroke(), Stroke::Dashed);
    assert_eq!(LinkStyle::Skipped.stroke(), Stroke::Dashed);
    assert_eq!(LinkStyle::NotTaken.stroke(), Stroke::Dotted);
}

#[test]
fn keeps_the_way_out_of_a_running_stage_dashed() {
    assert_eq!(join_style(Done), LinkStyle::Done);
    assert_eq!(join_style(Current), LinkStyle::Upcoming);
    assert_eq!(join_style(Upcoming), LinkStyle::Upcoming);
    assert_eq!(join_style(Skipped), LinkStyle::Skipped);
    assert_eq!(join_style(NotTaken), LinkStyle::NotTaken);
}

#[test]
fn sums_the_status_of_a_group() {
    assert_eq!(group_status(&[Done, Current]), Current);
    assert_eq!(group_status(&[Done, Done]), Done);
    assert_eq!(group_status(&[Done, Skipped]), Done);
    assert_eq!(group_status(&[Skipped, Skipped]), Skipped);
    assert_eq!(group_status(&[NotTaken, NotTaken]), NotTaken);
    assert_eq!(group_status(&[Upcoming, Upcoming, Upcoming]), Upcoming);
    assert_eq!(group_status(&[]), Upcoming);
}

#[test]
fn gives_each_status_and_state_its_node_tone() {
    assert_eq!(node_tone(Done, None, false, false), NodeTone::Done);
    assert_eq!(
        node_tone(Current, Some(TaskState::Failed), false, false),
        NodeTone::Current {
            state: TaskState::Failed,
            checking: false,
        }
    );
    assert_eq!(
        node_tone(Current, None, false, true),
        NodeTone::Current {
            state: TaskState::Ready,
            checking: true,
        }
    );
    assert_eq!(node_tone(Upcoming, None, false, false), NodeTone::Upcoming);
    assert_eq!(node_tone(Upcoming, None, true, false), NodeTone::WillSkip);
    assert_eq!(node_tone(Skipped, None, false, false), NodeTone::Skipped);
    assert_eq!(node_tone(NotTaken, None, false, false), NodeTone::NotTaken);
}

#[test]
fn gives_each_tone_its_border_glow_and_overlay() {
    let running = NodeTone::Current {
        state: TaskState::Running,
        checking: false,
    };
    assert_eq!(running.border(), Stroke::Solid);
    assert!(running.glow() && !running.muted());
    assert_eq!(running.color_state(), Some(TaskState::Running));
    assert_eq!(NodeTone::Done.glyph_state(), Some(TaskState::Done));
    assert!(!NodeTone::Done.muted() && !NodeTone::Done.glow());
    assert_eq!(NodeTone::Upcoming.border(), Stroke::Dashed);
    assert_eq!(NodeTone::WillSkip.border(), Stroke::Dashed);
    assert_eq!(NodeTone::NotTaken.border(), Stroke::Dotted);
    assert!(NodeTone::Skipped.struck() && NodeTone::Skipped.muted());
    assert_eq!(NodeTone::Skipped.glyph_state(), Some(TaskState::Cancelled));
    assert_eq!(NodeTone::Skipped.color_state(), None);
    assert_eq!(NodeTone::Upcoming.glyph_state(), None);
}

#[test]
fn a_stage_marked_to_skip_fades_as_a_skipped_one_and_not_as_one_that_will_run() {
    assert!(NodeTone::WillSkip.struck());
    assert_eq!(NodeTone::WillSkip.fade(), NodeTone::Skipped.fade());
    assert!(NodeTone::WillSkip.fade() < 1.);
    assert!(!NodeTone::Upcoming.struck());
    assert_eq!(NodeTone::Upcoming.fade(), 1.);
    // It has not run: no skipped glyph, the dashed tile of a stage to come.
    assert_eq!(NodeTone::WillSkip.glyph_state(), None);
    assert_eq!(NodeTone::NotTaken.fade(), 0.55);
}

fn stage(id: &str) -> Stage {
    Stage::new(id, StageKind::Agent, Gate::Auto)
}

fn path(id: &str, when: bool, not: bool, stages: Vec<Stage>) -> BranchPath {
    BranchPath {
        id: id.into(),
        name: None,
        when: when.then(|| Condition::Command {
            run: "test -f x".into(),
        }),
        not: not.then_some(true),
        stages,
    }
}

#[test]
fn names_each_lane_by_its_place_its_not_and_its_condition() {
    assert_eq!(lane_keyword(0, &path("a", true, false, vec![])), "if");
    assert_eq!(lane_keyword(1, &path("b", true, false, vec![])), "else if");
    assert_eq!(lane_keyword(0, &path("c", true, true, vec![])), "unless");
    assert_eq!(lane_keyword(2, &path("d", true, true, vec![])), "unless");
    assert_eq!(
        lane_keyword(3, &path("e", false, false, vec![])),
        "otherwise"
    );
}

/// Paths: bug (reproduce), small (implement), feature (Otherwise: plan).
fn branch() -> StageBranch {
    StageBranch {
        id: "kind".into(),
        name: None,
        paths: vec![
            path("bug", true, false, vec![stage("reproduce")]),
            path("small", true, false, vec![stage("implement")]),
            path("feature", false, false, vec![stage("plan")]),
        ],
    }
}

/// Statuses of reproduce, implement, plan, in that order.
fn statuses(list: [StageStatus; 3]) -> impl Fn(&str) -> Option<StageStatus> {
    move |id| match id {
        "reproduce" => Some(list[0]),
        "implement" => Some(list[1]),
        "plan" => Some(list[2]),
        _ => None,
    }
}

#[test]
fn says_in_one_line_which_path_the_branch_took() {
    let branch = branch();
    let line = |list| branch_line(&branch, &statuses(list));
    assert_eq!(
        line([Upcoming, Upcoming, Upcoming]),
        "Picks a path when it gets here"
    );
    assert_eq!(line([Current, NotTaken, NotTaken]), "Taken: bug");
    assert_eq!(line([NotTaken, NotTaken, Done]), "Taken: feature");
    assert_eq!(line([NotTaken, NotTaken, NotTaken]), "No path was taken");
}

#[test]
fn opens_every_lane_before_the_branch_and_folds_the_ones_not_taken() {
    let branch = branch();
    let state = |list, index: usize| lane_state(&branch, &branch.paths[index], &statuses(list));
    assert_eq!(
        state([Upcoming, Upcoming, Upcoming], 0),
        LaneState::NotReached
    );
    assert_eq!(state([NotTaken, Current, NotTaken], 1), LaneState::Taken);
    assert_eq!(state([NotTaken, Current, NotTaken], 0), LaneState::NotTaken);
    assert_eq!(
        state([NotTaken, NotTaken, NotTaken], 2),
        LaneState::NotTaken
    );
}

#[test]
fn counts_the_stages_of_a_lane() {
    assert_eq!(stage_count_label(1), "1 stage");
    assert_eq!(stage_count_label(3), "3 stages");
}

#[test]
fn puts_two_stages_side_by_side_and_stacks_three_or_four() {
    assert_eq!(group_layout(2), GroupLayout::Pair);
    assert_eq!(group_layout(3), GroupLayout::Stack);
    assert_eq!(group_layout(4), GroupLayout::Stack);
}

#[test]
fn puts_the_tiles_of_a_pair_in_the_middle_of_each_node() {
    // The owner's picture (T-0259): each tile in the middle of its node. 300 wide, 8 apart:
    // nodes are 146 wide, at 0..146 and 154..300.
    assert_eq!(pair_tiles(300., 1.), (73., 227.));
    assert_eq!(pair_tiles(400., 1.), (98., 302.));
    // The UI size scales the gap.
    assert_eq!(pair_tiles(300., 1.5), (72., 228.));
    assert_eq!(pair_tiles(0., 1.), (0., 0.));
}

#[test]
fn the_caret_points_at_the_tile_of_the_selected_node() {
    assert_eq!(caret_x(CaretAt::Middle, 300., 1.), 150.);
    // The same x as the pair's tiles.
    assert_eq!(caret_x(CaretAt::Left, 300., 1.), 73.);
    assert_eq!(caret_x(CaretAt::Right, 300., 1.), 227.);
}

#[test]
fn keeps_the_tiles_of_a_column_24_apart() {
    // 12 px between two nodes, plus the 6 px stub inside each (T-0259).
    assert_eq!(LINK_H + 2. * STUB, 24.);
}
