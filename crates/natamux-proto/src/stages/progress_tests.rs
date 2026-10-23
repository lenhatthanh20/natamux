use super::*;
use crate::stages::catalog::default_pipeline;
use crate::stages::schema::{Condition, StageBranch};
use serde_json::json;

// Each event comes one minute after the one before, like the fixture of the TypeScript test.
struct Log {
    minute: u32,
}

impl Log {
    fn new() -> Log {
        Log { minute: 0 }
    }

    fn event(&mut self, kind: &str, payload: Value, source: &str) -> TaskEvent {
        self.minute += 1;
        TaskEvent {
            task_id: "T-1".into(),
            source: source.into(),
            kind: kind.into(),
            at: format!("2026-10-09T09:{:02}:00.000Z", self.minute),
            payload: payload.as_object().cloned().unwrap_or_default(),
        }
    }

    fn sys(&mut self, kind: &str, payload: Value) -> TaskEvent {
        self.event(kind, payload, "system")
    }

    fn human(&mut self, kind: &str, payload: Value) -> TaskEvent {
        self.event(kind, payload, "human")
    }
}

fn at(stage: &str, state: TaskState, lanes: Value) -> Task {
    Task {
        stage: stage.into(),
        state,
        lanes: lanes.as_object().cloned().unwrap_or_default(),
        ..Task::default()
    }
}

fn find<'a>(progress: &'a [StageProgress], stage: &str) -> &'a StageProgress {
    progress.iter().find(|each| each.stage == stage).unwrap()
}

fn statuses(progress: &[StageProgress]) -> Vec<(&str, StageStatus)> {
    progress
        .iter()
        .map(|each| (each.stage.as_str(), each.status))
        .collect()
}

#[test]
fn marks_stages_done_current_and_upcoming_with_times_rounds_and_evidence() {
    let mut log = Log::new();
    let events = [
        log.sys("stage.started", json!({"stage": "context"})),
        log.sys(
            "stage.checked",
            json!({"stage": "context", "passes": ["context.md was written"]}),
        ),
        log.human("stage.approved", json!({"stage": "context"})),
        log.sys("stage.started", json!({"stage": "spec"})),
        log.sys(
            "stage.check-failed",
            json!({"stage": "spec", "reasons": ["spec.md is missing"]}),
        ),
    ];
    let progress = stage_progress(
        &at("spec", TaskState::Running, json!({})),
        &events,
        &default_pipeline(),
    );
    use StageStatus::{Current, Done, Upcoming};
    assert_eq!(
        statuses(&progress),
        [
            ("context", Done),
            ("spec", Current),
            ("plan", Upcoming),
            ("test-spec", Upcoming),
            ("implement", Upcoming),
            ("testing", Upcoming),
            ("agent-review", Upcoming),
            ("release", Upcoming),
        ]
    );
    let context = &progress[0];
    assert_eq!(
        context.started_at.as_deref(),
        Some("2026-10-09T09:01:00.000Z")
    );
    assert_eq!(
        context.ended_at.as_deref(),
        Some("2026-10-09T09:03:00.000Z")
    );
    assert_eq!(context.approved_by.as_deref(), Some("human"));
    assert_eq!(context.passes, ["context.md was written"]);
    assert_eq!(context.rounds, 1);
    assert_eq!(progress[1].state, Some(TaskState::Running));
    assert_eq!(progress[1].reasons, ["spec.md is missing"]);
    assert_eq!(progress[3].group.as_deref(), Some("build"));
    assert_eq!(progress[3].name, "Test spec");
}

#[test]
fn shows_each_stage_of_a_group_on_its_own_and_counts_a_stage_sent_back() {
    let mut log = Log::new();
    let events = [
        log.sys("stage.started", json!({"stage": "implement"})),
        log.human(
            "stage.reopened",
            json!({"to": "implement", "reason": "Add the error cases"}),
        ),
        log.sys("stage.started", json!({"stage": "implement"})),
    ];
    let task = at(
        "build",
        TaskState::Review,
        json!({"test-spec": "review", "implement": "done"}),
    );
    let progress = stage_progress(&task, &events, &default_pipeline());
    let test_spec = find(&progress, "test-spec");
    assert_eq!(test_spec.status, StageStatus::Current);
    assert_eq!(test_spec.state, Some(TaskState::Review));
    let implement = find(&progress, "implement");
    assert_eq!(implement.status, StageStatus::Done);
    assert_eq!(implement.rounds, 2);
    assert_eq!(
        implement.sent_back_for.as_deref(),
        Some("Add the error cases")
    );
}

#[test]
fn reads_an_approval_from_review_and_a_request_for_changes_like_their_stage_events() {
    let mut log = Log::new();
    let events = [
        log.sys("stage.started", json!({"stage": "plan"})),
        log.sys("stage.checked", json!({"stage": "plan"})),
        log.human("review.approved", json!({"stage": "plan"})),
        log.sys("stage.started", json!({"stage": "implement"})),
        log.human(
            "review.changes-requested",
            json!({"to": "implement", "reason": "Fix the dates"}),
        ),
        log.sys("stage.started", json!({"stage": "implement"})),
    ];
    let task = at(
        "build",
        TaskState::Running,
        json!({"test-spec": "review", "implement": "running"}),
    );
    let progress = stage_progress(&task, &events, &default_pipeline());
    let plan = find(&progress, "plan");
    assert_eq!(plan.status, StageStatus::Done);
    assert_eq!(plan.ended_at.as_deref(), Some("2026-10-09T09:03:00.000Z"));
    assert_eq!(plan.approved_by.as_deref(), Some("human"));
    assert_eq!(
        find(&progress, "test-spec").started_at.as_deref(),
        Some("2026-10-09T09:05:00.000Z")
    );
    let implement = find(&progress, "implement");
    assert_eq!(implement.rounds, 2);
    assert_eq!(implement.sent_back_for.as_deref(), Some("Fix the dates"));
}

#[test]
fn tells_a_send_back_by_ci_from_one_by_a_review() {
    let mut log = Log::new();
    let task = at(
        "build",
        TaskState::Running,
        json!({"test-spec": "review", "implement": "running"}),
    );
    let implement_of = |events: &[TaskEvent]| {
        let progress = stage_progress(&task, events, &default_pipeline());
        let implement = find(&progress, "implement");
        (
            implement.rounds,
            implement.sent_back_for.clone(),
            implement.sent_back_by_ci,
        )
    };
    let by_ci = vec![
        log.sys("stage.started", json!({"stage": "implement"})),
        log.sys(
            "stage.reopened",
            json!({"to": "implement", "from": "implement", "reason": "CI failed on Implement:", "ci": true}),
        ),
        log.sys("stage.started", json!({"stage": "implement"})),
    ];
    assert_eq!(implement_of(&by_ci).0, 2);
    assert!(implement_of(&by_ci).2);

    let mut by_review = by_ci.clone();
    by_review.push(log.human(
        "review.changes-requested",
        json!({"to": "implement", "reason": "Fix the dates"}),
    ));
    by_review.push(log.sys("stage.started", json!({"stage": "implement"})));
    assert_eq!(
        (implement_of(&by_review).0, implement_of(&by_review).2),
        (3, false)
    );

    let mut review_then_ci = by_review.clone();
    review_then_ci.push(log.sys(
        "stage.reopened",
        json!({"to": "implement", "from": "implement", "reason": "CI", "ci": true}),
    ));
    review_then_ci.push(log.sys("stage.started", json!({"stage": "implement"})));
    assert_eq!(
        (
            implement_of(&review_then_ci).0,
            implement_of(&review_then_ci).2
        ),
        (4, true)
    );

    assert_eq!(implement_of(&by_ci[..1]), (1, None, false));
    let old = vec![
        log.sys("stage.started", json!({"stage": "implement"})),
        log.sys(
            "stage.reopened",
            json!({"to": "implement", "reason": "Before CI send backs"}),
        ),
        log.sys("stage.started", json!({"stage": "implement"})),
    ];
    assert_eq!((implement_of(&old).0, implement_of(&old).2), (2, false));
}

#[test]
fn marks_a_skipped_stage_and_every_stage_of_a_done_task() {
    let mut log = Log::new();
    let events = [log.sys("stage.skipped", json!({"stage": "context"}))];
    let progress = stage_progress(
        &at("spec", TaskState::Ready, json!({})),
        &events,
        &default_pipeline(),
    );
    assert_eq!(progress[0].status, StageStatus::Skipped);

    let done = stage_progress(
        &at("release", TaskState::Done, json!({})),
        &[],
        &default_pipeline(),
    );
    assert!(done.iter().all(|each| each.status == StageStatus::Done));
}

#[test]
fn starts_a_stage_that_waits_for_a_person_when_the_step_before_it_ended() {
    let mut log = Log::new();
    let events = [
        log.sys("stage.started", json!({"stage": "testing"})),
        log.sys("stage.checked", json!({"stage": "testing"})),
    ];
    let steps = [
        Step::Stage(Stage::new("testing", StageKind::Testing, Gate::Auto)),
        Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
    ];
    let progress = stage_progress(&at("release", TaskState::Ready, json!({})), &events, &steps);
    assert_eq!(progress[1].status, StageStatus::Current);
    assert_eq!(
        progress[1].started_at.as_deref(),
        Some("2026-10-09T09:02:00.000Z")
    );
}

// A branch: plan, then a small path or a PR review path, then release.
fn branch_steps() -> Vec<Step> {
    let mut plan = Stage::new("plan", StageKind::Agent, Gate::Manual);
    plan.output = Some("plan.md".into());
    vec![
        Step::Stage(plan),
        Step::Branch(StageBranch {
            id: "after-plan".into(),
            name: None,
            paths: vec![
                BranchPath {
                    id: "small".into(),
                    name: None,
                    when: Some(Condition::Output {
                        file: "plan.md".into(),
                        section: Some("Size".into()),
                        contains: "small".into(),
                    }),
                    not: None,
                    stages: vec![
                        Stage::new("implement", StageKind::Agent, Gate::Auto),
                        Stage::new("testing", StageKind::Testing, Gate::Auto),
                    ],
                },
                BranchPath {
                    id: "pr-review".into(),
                    name: None,
                    when: None,
                    not: None,
                    stages: vec![Stage::new("review", StageKind::Review, Gate::Manual)],
                },
            ],
        }),
        Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
    ]
}

fn branched(stage: &str, state: TaskState, paths: Value) -> Task {
    Task {
        paths: paths.as_object().cloned().unwrap_or_default(),
        ..at(stage, state, json!({}))
    }
}

#[test]
fn marks_the_stages_of_a_path_not_taken_and_gives_each_stage_its_path_and_reason() {
    let because = "plan.md \"Size\" contains \"small\"";
    let mut log = Log::new();
    let events = [
        log.sys("stage.started", json!({"stage": "plan"})),
        log.sys("stage.checked", json!({"stage": "plan"})),
        log.human(
            "stage.approved",
            json!({"stage": "plan", "path": "small", "because": because}),
        ),
        log.sys("stage.started", json!({"stage": "implement"})),
        log.sys("stage.checked", json!({"stage": "implement"})),
    ];
    let task = branched("testing", TaskState::Ready, json!({"after-plan": "small"}));
    let progress = stage_progress(&task, &events, &branch_steps());
    use StageStatus::{Current, Done, NotTaken, Upcoming};
    let rows: Vec<(&str, StageStatus, Option<&str>)> = progress
        .iter()
        .map(|each| (each.stage.as_str(), each.status, each.path.as_deref()))
        .collect();
    assert_eq!(
        rows,
        [
            ("plan", Done, None),
            ("implement", Done, Some("small")),
            ("testing", Current, Some("small")),
            ("review", NotTaken, Some("pr-review")),
            ("release", Upcoming, None),
        ]
    );
    let reasons: Vec<Option<&str>> = progress
        .iter()
        .map(|each| each.because.as_deref())
        .collect();
    assert_eq!(reasons, [None, Some(because), None, None, None]);
    assert_eq!(
        progress[2].started_at.as_deref(),
        Some("2026-10-09T09:05:00.000Z")
    );
    assert_eq!(progress[2].rounds, 1);
    assert_eq!(progress[3].started_at, None);
    assert_eq!(progress[3].ended_at, None);
    assert_eq!(progress[3].rounds, 0);
}

#[test]
fn starts_the_first_stage_of_the_path_taken_when_the_event_that_chose_it_came() {
    let mut log = Log::new();
    let events = [
        log.sys("stage.started", json!({"stage": "plan"})),
        log.sys(
            "stage.approved",
            json!({"stage": "plan", "path": "pr-review", "because": "Otherwise"}),
        ),
    ];
    let task = branched(
        "review",
        TaskState::Review,
        json!({"after-plan": "pr-review"}),
    );
    let progress = stage_progress(&task, &events, &branch_steps());
    let review = find(&progress, "review");
    assert_eq!(review.status, StageStatus::Current);
    assert_eq!(
        review.started_at.as_deref(),
        Some("2026-10-09T09:02:00.000Z")
    );
    assert_eq!(review.because.as_deref(), Some("Otherwise"));
    let not_taken = progress
        .iter()
        .filter(|each| each.status == StageStatus::NotTaken)
        .count();
    assert_eq!(not_taken, 2);
}

#[test]
fn shows_every_stage_of_a_skipped_branch_as_not_taken_and_all_as_upcoming_before_it() {
    let mut log = Log::new();
    let events = [log.sys(
        "stage.approved",
        json!({"stage": "plan", "because": "No condition held"}),
    )];
    for state in [TaskState::Ready, TaskState::Done] {
        let task = branched("release", state, json!({"after-plan": ""}));
        let progress = stage_progress(&task, &events, &branch_steps());
        let last = if state == TaskState::Done {
            StageStatus::Done
        } else {
            StageStatus::Current
        };
        let all: Vec<StageStatus> = progress.iter().map(|each| each.status).collect();
        assert_eq!(
            all,
            [
                StageStatus::Done,
                StageStatus::NotTaken,
                StageStatus::NotTaken,
                StageStatus::NotTaken,
                last
            ]
        );
        assert!(progress.iter().all(|each| each.because.is_none()));
    }
    // No choice yet: a record of {} and no record at all read the same.
    for paths in [json!({}), Value::Null] {
        let task = branched("plan", TaskState::Running, paths);
        let before = stage_progress(&task, &[], &branch_steps());
        assert!(
            before[1..]
                .iter()
                .all(|each| each.status == StageStatus::Upcoming)
        );
    }
}
