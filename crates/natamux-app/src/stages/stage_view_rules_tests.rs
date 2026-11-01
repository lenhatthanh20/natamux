use serde_json::{Map, Value, json};

use super::*;

fn entry(status: StageStatus) -> StageProgress {
    StageProgress {
        stage: "x".into(),
        name: "X".into(),
        kind: StageKind::Agent,
        gate: Gate::Auto,
        output: None,
        group: None,
        path: None,
        because: None,
        status,
        state: None,
        rounds: 0,
        started_at: None,
        ended_at: None,
        approved_by: None,
        passes: vec![],
        reasons: vec![],
        sent_back_for: None,
        sent_back_by_ci: false,
    }
}

fn at(path: Option<&str>) -> StageProgress {
    StageProgress {
        path: path.map(String::from),
        ..entry(StageStatus::Done)
    }
}

fn ms(text: &str) -> i64 {
    parse_iso_ms(text).unwrap()
}

fn event(kind: &str, stage: &str, extra: Value) -> TaskEvent {
    let mut payload = Map::new();
    payload.insert("stage".into(), json!(stage));
    if let Value::Object(map) = extra {
        payload.extend(map);
    }
    TaskEvent {
        task_id: "T-1".into(),
        source: "system".into(),
        kind: kind.into(),
        at: "2026-10-09T09:00:00.000Z".into(),
        payload,
    }
}

#[test]
fn says_each_time_span_in_words() {
    let table = [
        ("2026-10-09T09:00:00Z", "2026-10-09T09:00:40Z", "<1m"),
        ("2026-10-09T09:00:00Z", "2026-10-09T09:04:00Z", "4m"),
        ("2026-10-09T09:00:00Z", "2026-10-09T10:05:00Z", "1h 5m"),
        ("2026-10-09T09:00:00Z", "2026-10-09T11:00:00Z", "2h"),
        ("2026-10-09T09:00:00Z", "2026-10-11T12:00:00Z", "2d 3h"),
    ];
    for (from, to, text) in table {
        assert_eq!(format_duration_to(from, to), text, "{from} to {to}");
    }
}

#[test]
fn rounds_to_the_nearest_minute_and_never_goes_below_zero() {
    assert_eq!(
        format_duration_to("2026-10-09T09:00:00Z", "2026-10-09T09:01:30Z"),
        "2m"
    );
    assert_eq!(
        format_duration_to("2026-10-09T09:00:00Z", "2026-10-09T08:00:00Z"),
        "<1m"
    );
    assert_eq!(format_duration_to("nope", "2026-10-09T08:00:00Z"), "");
}

fn plan() -> Stage {
    Stage {
        output: Some("plan.md".into()),
        sections: Some(vec!["Steps".into(), "Files".into(), "Tests".into()]),
        ..Stage::new("plan", StageKind::Agent, Gate::Manual)
    }
}

#[test]
fn says_what_finishes_a_stage_from_its_checks() {
    assert_eq!(
        finished_when(&plan()),
        "Finished when plan.md is written, it has Steps, Files, Tests, and you approve it."
    );
    assert_eq!(
        finished_when(&Stage::new("release", StageKind::Release, Gate::Manual)),
        "Finished when the pull request is merged."
    );
    let own = Stage {
        done_when: Some("Legal signs off.".into()),
        ..Stage::new("x", StageKind::Manual, Gate::Manual)
    };
    assert_eq!(finished_when(&own), "Legal signs off.");
}

#[test]
fn says_what_finishes_the_other_kinds_of_stage() {
    let context = Stage::new("context", StageKind::Context, Gate::Auto);
    assert_eq!(
        finished_when(&context),
        "Finished when context.md is written."
    );
    let review = Stage::new("review", StageKind::Review, Gate::Manual);
    assert_eq!(
        finished_when(&review),
        "Finished when you approve the changes."
    );
    let manual = Stage::new("m", StageKind::Manual, Gate::Manual);
    assert_eq!(finished_when(&manual), "Finished when you approve it.");
    let testing = Stage::new("t", StageKind::Testing, Gate::Auto);
    assert_eq!(
        finished_when(&testing),
        "Finished when the tests are written and pass."
    );
    let command = Stage {
        command: Some("pnpm test".into()),
        ..testing
    };
    assert_eq!(finished_when(&command), "Finished when pnpm test passes.");
    let bare = Stage::new("a", StageKind::Agent, Gate::Manual);
    assert_eq!(
        finished_when(&bare),
        "Finished when the agent says it is done, and you approve it."
    );
}

#[test]
fn names_a_commit_only_when_the_stage_needs_one() {
    let implement = Stage {
        check: Some("pnpm test".into()),
        ..Stage::new("implement", StageKind::Agent, Gate::Auto)
    };
    assert!(!finished_when(&implement).contains("committed"));
    let committing = Stage {
        require_commit: Some(true),
        ..implement
    };
    assert!(finished_when(&committing).contains("the work is committed"));
}

#[test]
fn is_green_behind_the_work_moving_into_a_running_stage_dashed_ahead() {
    let done = entry(StageStatus::Done);
    let running = StageProgress {
        state: Some(TaskState::Running),
        ..entry(StageStatus::Current)
    };
    let upcoming = entry(StageStatus::Upcoming);
    assert_eq!(link_kind(Some(&done), Some(&done)), LinkKind::Done);
    assert_eq!(link_kind(Some(&done), Some(&running)), LinkKind::Flow);
    assert_eq!(
        link_kind(Some(&running), Some(&upcoming)),
        LinkKind::Upcoming
    );
    assert_eq!(link_kind(None, Some(&done)), LinkKind::Hidden);
}

#[test]
fn draws_a_dashed_line_next_to_a_stage_that_is_not_taken() {
    let done = entry(StageStatus::Done);
    let not_taken = entry(StageStatus::NotTaken);
    assert_eq!(link_kind(Some(&done), Some(&not_taken)), LinkKind::Upcoming);
    assert_eq!(link_kind(Some(&not_taken), Some(&done)), LinkKind::Upcoming);
}

#[test]
fn counts_done_and_skipped_stages_as_finished() {
    let list = [
        entry(StageStatus::Done),
        entry(StageStatus::Skipped),
        entry(StageStatus::Current),
    ];
    assert_eq!(stage_count(&list), (2, 3));
}

#[test]
fn leaves_out_the_stages_of_a_path_not_taken() {
    let list = [
        entry(StageStatus::Done),
        entry(StageStatus::NotTaken),
        entry(StageStatus::NotTaken),
        entry(StageStatus::Current),
    ];
    assert_eq!(stage_count(&list), (1, 2));
}

fn stage(id: &str) -> Stage {
    Stage::new(id, StageKind::Agent, Gate::Auto)
}

fn branch() -> StageBranch {
    let path = |id: &str, name: Option<&str>, stages: Vec<Stage>| BranchPath {
        id: id.into(),
        name: name.map(String::from),
        when: None,
        not: None,
        stages,
    };
    StageBranch {
        id: "after-plan".into(),
        name: None,
        paths: vec![
            path(
                "small",
                Some("Small change"),
                vec![stage("implement"), stage("testing")],
            ),
            path("big", None, vec![stage("review")]),
        ],
    }
}

/// Statuses of implement, testing, review, in that order.
fn statuses(list: [StageStatus; 3]) -> impl Fn(&str) -> Option<StageStatus> {
    move |id| match id {
        "implement" => Some(list[0]),
        "testing" => Some(list[1]),
        "review" => Some(list[2]),
        _ => None,
    }
}

#[test]
fn finds_the_path_the_task_took_by_its_stages_that_are_not_waiting() {
    use StageStatus::*;
    let branch = branch();
    let lookup = statuses([Current, Upcoming, NotTaken]);
    let taken = taken_path(&branch, &lookup);
    assert_eq!(taken.map(|path| path.id.as_str()), Some("small"));
}

#[test]
fn finds_no_path_before_the_branch_is_reached_and_none_when_it_was_skipped() {
    use StageStatus::*;
    let branch = branch();
    assert!(taken_path(&branch, &statuses([Upcoming, Upcoming, Upcoming])).is_none());
    assert!(taken_path(&branch, &statuses([NotTaken, NotTaken, NotTaken])).is_none());
}

#[test]
fn gives_the_status_of_the_diamond() {
    use StageStatus::*;
    let branch = branch();
    let status = |list| branch_status(&branch, &statuses(list));
    assert_eq!(status([Upcoming, Upcoming, Upcoming]), Upcoming);
    assert_eq!(status([Done, Current, NotTaken]), Current);
    assert_eq!(status([Done, Done, NotTaken]), Done);
    assert_eq!(status([NotTaken, NotTaken, NotTaken]), NotTaken);
}

#[test]
fn names_a_path_by_its_name_or_its_id() {
    let branch = branch();
    assert_eq!(path_label(&branch.paths[0]), "Small change");
    assert_eq!(path_label(&branch.paths[1]), "big");
}

#[test]
fn draws_no_line_from_a_stage_of_one_path_to_a_stage_of_another() {
    assert!(same_lane(Some(&at(None)), Some(&at(Some("small")))));
    assert!(same_lane(Some(&at(Some("small"))), Some(&at(None))));
    assert!(same_lane(
        Some(&at(Some("small"))),
        Some(&at(Some("small")))
    ));
    assert!(!same_lane(Some(&at(Some("small"))), Some(&at(Some("big")))));
    assert!(same_lane(None, Some(&at(Some("big")))));
}

fn lane(started: Option<&str>, ended: Option<&str>, status: StageStatus) -> StageProgress {
    StageProgress {
        started_at: started.map(String::from),
        ended_at: ended.map(String::from),
        ..entry(status)
    }
}

const NOW: &str = "2026-10-09T10:00:00Z";

#[test]
fn spans_from_the_first_start_to_the_last_end_of_side_by_side_stages() {
    let a = lane(
        Some("2026-10-09T09:00:00Z"),
        Some("2026-10-09T09:10:00Z"),
        StageStatus::Done,
    );
    let b = lane(
        Some("2026-10-09T09:02:00Z"),
        Some("2026-10-09T09:30:00Z"),
        StageStatus::Done,
    );
    assert_eq!(
        step_time(&[Some(&a), Some(&b)], ms(NOW)).as_deref(),
        Some("30m")
    );
}

#[test]
fn counts_up_to_now_while_a_stage_runs() {
    let a = lane(Some("2026-10-09T09:00:00Z"), None, StageStatus::Current);
    assert_eq!(
        step_time(&[Some(&a)], ms(NOW)).as_deref(),
        Some("1h so far")
    );
}

#[test]
fn is_null_before_a_start_or_when_a_finished_stage_has_no_end() {
    let upcoming = lane(None, None, StageStatus::Upcoming);
    assert_eq!(step_time(&[Some(&upcoming), None], ms(NOW)), None);
    let open = lane(Some("2026-10-09T09:00:00Z"), None, StageStatus::Done);
    assert_eq!(step_time(&[Some(&open)], ms(NOW)), None);
}

#[test]
fn says_the_time_of_one_stage() {
    let done = lane(
        Some("2026-10-09T09:00:00Z"),
        Some("2026-10-09T09:04:00Z"),
        StageStatus::Done,
    );
    assert_eq!(stage_time(&done, ms(NOW)).as_deref(), Some("4m"));
    let current = lane(Some("2026-10-09T09:00:00Z"), None, StageStatus::Current);
    assert_eq!(stage_time(&current, ms(NOW)).as_deref(), Some("1h so far"));
    assert_eq!(
        stage_time(&lane(None, None, StageStatus::Upcoming), ms(NOW)),
        None
    );
    assert_eq!(
        stage_time(
            &lane(Some("2026-10-09T09:00:00Z"), None, StageStatus::Done),
            ms(NOW)
        ),
        None
    );
}

#[test]
fn keeps_the_skills_of_the_latest_start_of_each_stage() {
    let events = [
        event(
            "stage.started",
            "spec",
            json!({"missingSkills": ["a", "b"], "agent": "codex"}),
        ),
        event(
            "stage.started",
            "plan",
            json!({"missingSkills": ["c"], "agent": "codex"}),
        ),
        event("stage.started", "spec", json!({})),
    ];
    let missing = missing_skills(&events);
    let get = |id: &str| {
        missing
            .iter()
            .find(|(name, _)| name == id)
            .map(|(_, v)| v.clone())
    };
    assert_eq!(get("spec"), Some(MissingSkills::default()));
    assert_eq!(
        get("plan"),
        Some(MissingSkills {
            names: vec!["c".into()],
            agent: Some("codex".into())
        })
    );
}

#[test]
fn names_the_stages_whose_ci_check_runs_now_with_their_ci_branch() {
    let ci = json!({"branch": "natamux-ci/a"});
    let events = [
        event("stage.ci-started", "implement", ci.clone()),
        event("stage.checked", "implement", json!({})),
        event("stage.ci-started", "testing", ci.clone()),
        event("stage.ci-started", "docs", ci),
        event("stage.check-failed", "docs", json!({})),
    ];
    assert_eq!(
        waiting_for_ci(&events),
        [("testing".to_string(), "natamux-ci/a".to_string())]
    );
}

#[test]
fn names_the_stages_whose_last_start_asked_for_a_workflow() {
    let workflow = json!({"workflow": true});
    let events = [
        event("stage.started", "plan", workflow.clone()),
        event("stage.started", "implement", workflow),
        event("stage.started", "implement", json!({})),
        event("stage.started", "test-spec", json!({})),
    ];
    assert_eq!(ran_with_workflow(&events), ["plan"]);
}

#[test]
fn finds_a_stage_of_a_group_or_a_path() {
    let steps = [Step::Stage(stage("spec")), Step::Branch(branch())];
    assert_eq!(
        find_stage(&steps, "testing").map(|s| s.id.as_str()),
        Some("testing")
    );
    assert!(find_stage(&steps, "nope").is_none());
}

#[test]
fn shows_a_done_stage_as_a_disk_with_a_check() {
    assert_eq!(marker_color_state(StageStatus::Done, None), "done");
    assert_eq!(marker_shape(StageStatus::Done, None), Some("done"));
}

#[test]
fn shows_a_running_stage_with_the_running_shape_and_nothing_else_moves() {
    assert_eq!(
        marker_shape(StageStatus::Current, Some(TaskState::Running)),
        Some("running")
    );
}

#[test]
fn puts_the_shape_of_each_state_inside_the_current_stage() {
    for state in [
        TaskState::NeedsInput,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Ready,
    ] {
        assert_eq!(
            marker_color_state(StageStatus::Current, Some(state)),
            state.name()
        );
        assert_eq!(
            marker_shape(StageStatus::Current, Some(state)),
            Some(state.name())
        );
    }
    assert_eq!(marker_shape(StageStatus::Current, None), Some("ready"));
}

#[test]
fn shows_a_skipped_stage_with_the_dash_and_an_upcoming_one_empty() {
    assert_eq!(marker_shape(StageStatus::Skipped, None), Some("cancelled"));
    assert_eq!(marker_shape(StageStatus::Upcoming, None), None);
}

#[test]
fn shows_a_stage_of_a_path_not_taken_as_an_empty_dashed_node_that_says_not_taken() {
    assert_eq!(marker_shape(StageStatus::NotTaken, None), None);
    assert_eq!(marker_color_state(StageStatus::NotTaken, None), "draft");
    assert_eq!(marker_label(StageStatus::NotTaken, None), "Not taken");
}

#[test]
fn says_the_label_of_each_status_and_of_the_state_of_a_current_stage() {
    assert_eq!(marker_label(StageStatus::Done, None), "Done");
    assert_eq!(marker_label(StageStatus::Current, None), "Now");
    assert_eq!(marker_label(StageStatus::Upcoming, None), "Not reached");
    assert_eq!(marker_label(StageStatus::Skipped, None), "Skipped");
    assert_eq!(
        marker_label(StageStatus::Current, Some(TaskState::Review)),
        "In review"
    );
}

#[test]
fn draws_the_inner_shape_on_whole_even_pixels_at_every_node_size() {
    for size in [14.0, 18.0, 20.0, 24.0] {
        for share in [0.5, 0.4] {
            let art = even_px(size, share);
            assert_eq!(art % 2.0, 0.0, "{size} at {share}");
        }
    }
    assert_eq!(even_px(20.0, 0.4), 8.0);
}
