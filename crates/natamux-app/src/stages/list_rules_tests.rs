use natamux_proto::stages::schema::{
    BranchPath, Condition, Gate, Stage, StageGroup, StageKind, Step,
};

use super::*;
use crate::components::colors::TileColor;
use crate::stages::editor_find::find_stage;
use crate::stages::editor_fixtures::{branched, stage_step};

fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
    Stage::new(id, kind, gate)
}

#[test]
fn a_tile_has_the_colour_of_its_kind_and_a_stage_of_your_own_is_cyan() {
    assert_eq!(
        kind_tile(&stage("spec", StageKind::Agent, Gate::Manual)),
        TileColor::Blue
    );
    assert_eq!(
        kind_tile(&stage("testing", StageKind::Testing, Gate::Auto)),
        TileColor::Green
    );
    assert_eq!(
        kind_tile(&stage("qa", StageKind::Manual, Gate::Manual)),
        TileColor::Yellow
    );
    assert_eq!(
        kind_tile(&stage("release", StageKind::Release, Gate::Manual)),
        TileColor::Violet
    );
    assert_eq!(
        kind_tile(&stage("perf-budget", StageKind::Testing, Gate::Manual)),
        TileColor::Cyan
    );
}

#[test]
fn the_meta_says_the_agent_the_command_of_a_testing_stage_and_whether_it_waits() {
    let mut own = stage("implement", StageKind::Agent, Gate::Auto);
    own.agent = Some("codex".into());
    assert_eq!(row_meta(&own), "Codex \u{b7} Auto");
    let mut test = stage("testing", StageKind::Testing, Gate::Auto);
    test.command = Some("pnpm build && pnpm test".into());
    assert_eq!(row_meta(&test), "pnpm build && pnpm test \u{b7} Auto");
    assert_eq!(
        row_meta(&stage("spec", StageKind::Agent, Gate::Manual)),
        "Waits for you"
    );
}

#[test]
fn an_agent_command_is_not_in_the_meta_and_a_manual_stage_always_waits() {
    let mut agent = stage("implement", StageKind::Agent, Gate::Auto);
    agent.command = Some("make".into());
    assert_eq!(row_meta(&agent), "Auto");
    assert_eq!(
        row_meta(&stage("qa", StageKind::Manual, Gate::Auto)),
        "Waits for you"
    );
}

#[test]
fn a_group_without_a_name_reads_side_by_side() {
    let mut group = StageGroup {
        id: "g".into(),
        name: None,
        parallel: vec![],
    };
    assert_eq!(group_caption(&group), "Side by side");
    group.name = Some("Checks".into());
    assert_eq!(group_caption(&group), "Checks side by side");
}

#[test]
fn a_path_caption_says_its_condition() {
    let path = |when: Option<Condition>, not: Option<bool>| BranchPath {
        id: "p".into(),
        name: None,
        when,
        not,
        stages: vec![],
    };
    assert_eq!(path_condition(&path(None, None)), "Otherwise");
    let run = Condition::Command {
        run: "test -f .no-review".into(),
    };
    assert_eq!(
        path_condition(&path(Some(run), None)),
        "if test -f .no-review succeeds"
    );
    let file = Condition::Output {
        file: "plan.md".into(),
        section: Some("Size".into()),
        contains: "small".into(),
    };
    assert_eq!(
        path_condition(&path(Some(file.clone()), None)),
        "if plan.md \u{203a} Size has \"small\""
    );
    assert_eq!(
        path_condition(&path(Some(file), Some(true))),
        "if plan.md \u{203a} Size does not have \"small\""
    );
}

#[test]
fn counts_the_stages_in_groups_and_paths_and_names_the_paths_of_a_branch() {
    let steps = branched();
    assert_eq!(stage_count(&steps), 6);
    let Some(Step::Branch(branch)) = steps.get(2) else {
        panic!("no branch");
    };
    assert_eq!(branch_meta(branch), "2 paths, checked in order");
}

#[test]
fn the_first_stage_cannot_move_up_and_the_last_cannot_move_down() {
    let steps = vec![
        stage_step("spec", StageKind::Agent, Gate::Manual),
        stage_step("plan", StageKind::Agent, Gate::Manual),
        stage_step("testing", StageKind::Testing, Gate::Auto),
    ];
    let menu = |id: &str| stage_menu(&steps, find_stage(&steps, id).unwrap());
    let first = menu("spec");
    assert!(!first.up && first.down);
    assert!(matches!(first.join, JoinItem::Alongside(Some(_))));
    let second = menu("plan");
    assert_eq!(second.join, JoinItem::Alongside(None));
    let last = menu("testing");
    assert!(last.up && !last.down);
    assert_eq!(last.join, JoinItem::None);
}

#[test]
fn a_stage_in_a_group_can_run_on_its_own() {
    let steps = vec![Step::Group(StageGroup {
        id: "g".into(),
        name: None,
        parallel: vec![
            stage("spec", StageKind::Agent, Gate::Manual),
            stage("plan", StageKind::Agent, Gate::Manual),
        ],
    })];
    let menu = stage_menu(&steps, find_stage(&steps, "plan").unwrap());
    assert_eq!(menu.join, JoinItem::Alone);
}

#[test]
fn a_tile_glyph_follows_the_kind_and_agent_kinds_share_the_bot() {
    assert_eq!(kind_tile_icon(StageKind::Agent), "bot");
    assert_eq!(kind_tile_icon(StageKind::Context), "bot");
    assert_eq!(kind_tile_icon(StageKind::Review), "bot");
    assert_eq!(kind_tile_icon(StageKind::Testing), "flask-conical");
    assert_eq!(kind_tile_icon(StageKind::Manual), "hand");
    assert_eq!(kind_tile_icon(StageKind::Release), "send");
}
