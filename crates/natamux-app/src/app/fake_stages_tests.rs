use natamux_proto::stages::progress::{StageProgress, StageStatus, stage_progress};

use super::*;

fn accepted(fake: &FakeDaemon, id: &str) -> Vec<TaskEvent> {
    let events = fake.list_events(id).unwrap();
    events
        .into_iter()
        .filter(|stored| stored.accepted)
        .map(|stored| stored.event)
        .collect()
}

fn progress_of(fake: &FakeDaemon, id: &str) -> (Task, Vec<StageProgress>) {
    let task = fake
        .data()
        .tasks
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .unwrap();
    let settings = fake.load_settings("natamux").unwrap();
    let steps = resolve_steps(&steps_of(&task), &settings.settings.stages);
    let progress = stage_progress(&task, &accepted(fake, id), &steps);
    (task, progress)
}

fn current(progress: &[StageProgress]) -> Vec<(&str, Option<TaskState>)> {
    progress
        .iter()
        .filter(|entry| entry.status == StageStatus::Current)
        .map(|entry| (entry.stage.as_str(), entry.state))
        .collect()
}

#[test]
fn the_settings_hold_the_reference_pipeline() {
    let settings = FakeDaemon::default().load_settings("natamux").unwrap();
    let ids: Vec<&str> = settings.settings.stages.iter().map(Step::id).collect();
    assert_eq!(
        ids,
        [
            "context",
            "spec",
            "kind",
            "build",
            "agent-review",
            "security",
            "review",
            "release"
        ]
    );
    assert_eq!(settings.settings.tracker.kind, "github");
}

#[test]
fn the_review_task_is_at_manual_review_after_two_rounds() {
    let fake = FakeDaemon::default();
    let (task, progress) = progress_of(&fake, "T-0143");
    assert_eq!(current(&progress), [("review", Some(TaskState::Review))]);
    let round = |id: &str| progress.iter().find(|e| e.stage == id).map(|e| e.rounds);
    assert_eq!(round("implement"), Some(2));
    assert_eq!(round("agent-review"), Some(2));
    let bug = progress.iter().find(|e| e.stage == "reproduce").unwrap();
    assert_eq!(bug.status, StageStatus::Done);
    assert!(
        bug.because
            .as_deref()
            .is_some_and(|b| b.contains("grep -qi bug"))
    );
    let plan = progress.iter().find(|e| e.stage == "plan").unwrap();
    assert_eq!(plan.status, StageStatus::NotTaken);
    assert_eq!(task.priority, "high");
    assert_eq!(task.labels, ["bug"]);
    assert_eq!(fake.data().usage["T-0143"].stages.len(), 4);
}

#[test]
fn the_asking_task_waits_at_spec() {
    let fake = FakeDaemon::default();
    let (_, progress) = progress_of(&fake, "T-0141");
    assert_eq!(current(&progress), [("spec", Some(TaskState::NeedsInput))]);
}

#[test]
fn the_feature_task_waits_for_approval_of_its_plan() {
    let fake = FakeDaemon::default();
    let (_, progress) = progress_of(&fake, "T-0150");
    assert_eq!(current(&progress), [("plan", Some(TaskState::Review))]);
    let plan = progress.iter().find(|e| e.stage == "plan").unwrap();
    assert_eq!(plan.because.as_deref(), Some("Otherwise"));
    assert_eq!(
        plan.passes,
        ["plan.md is written", "plan.md has Steps, Files, Tests"]
    );
    assert_eq!(
        progress
            .iter()
            .find(|e| e.stage == "reproduce")
            .unwrap()
            .status,
        StageStatus::NotTaken
    );
    let files = fake.list_stage_files("T-0150").unwrap();
    assert_eq!(
        files.iter().find(|f| f.name == "plan.md").map(|f| f.size),
        Some(110)
    );
}

#[test]
fn the_failed_task_says_why() {
    let fake = FakeDaemon::default();
    let (_, progress) = progress_of(&fake, "T-0151");
    assert_eq!(current(&progress), [("plan", Some(TaskState::Failed))]);
    let plan = progress.iter().find(|e| e.stage == "plan").unwrap();
    assert_eq!(
        plan.reasons,
        ["The Plan agent stopped: it could not read src/catalog.ts."]
    );
}

#[test]
fn the_tries_share_a_label_and_have_diff_stats() {
    let fake = FakeDaemon::default();
    let data = fake.data();
    let tries: Vec<&Task> = data
        .tasks
        .iter()
        .filter(|t| t.labels.contains(&"tries-t-0135".to_string()))
        .collect();
    assert_eq!(tries.len(), 3);
    assert_eq!(
        data.diff_stats["T-0152"],
        DiffStat {
            files: 4,
            added: 8,
            removed: 0
        }
    );
    assert_eq!(data.diff_stats["T-0153"].files, 3);
}

#[test]
fn approving_moves_to_the_next_stage_and_logs_the_event() {
    let fake = FakeDaemon::default();
    let (mut feed, _) = fake.watch();
    while feed.try_recv().is_ok() {}
    let input = StageActionInput::new(StageAction::Approve);
    let task = fake.stage_action("T-0150", input).unwrap();
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("test-spec", TaskState::Running)
    );
    let events = accepted(&fake, "T-0150");
    assert!(
        events
            .iter()
            .any(|e| e.kind == "stage.approved" && e.at == NOW)
    );
    assert_eq!(
        events.last().map(|e| e.kind.as_str()),
        Some("stage.started")
    );
    assert!(feed.try_recv().is_ok());
}

#[test]
fn approving_the_manual_review_moves_to_release_and_the_last_stage_ends_the_task() {
    let fake = FakeDaemon::default();
    let task = fake
        .stage_action("T-0143", StageActionInput::new(StageAction::Approve))
        .unwrap();
    assert_eq!(task.stage, "release");
    assert!(
        accepted(&fake, "T-0143")
            .iter()
            .any(|e| e.kind == "review.approved")
    );
    let task = fake
        .stage_action("T-0143", StageActionInput::new(StageAction::Approve))
        .unwrap();
    assert_eq!(task.state, TaskState::Done);
}

#[test]
fn skipping_marks_the_stage_skipped() {
    let fake = FakeDaemon::default();
    fake.stage_action("T-0141", StageActionInput::new(StageAction::Skip))
        .unwrap();
    let (task, progress) = progress_of(&fake, "T-0141");
    assert_eq!(task.stage, "reproduce");
    assert_eq!(
        progress.iter().find(|e| e.stage == "spec").unwrap().status,
        StageStatus::Skipped
    );
}

#[test]
fn reporting_goes_to_review_for_a_manual_gate_and_on_for_an_auto_one() {
    let fake = FakeDaemon::default();
    let task = fake
        .stage_action("T-0141", StageActionInput::new(StageAction::Report))
        .unwrap();
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Review)
    );
    let mut input = StageActionInput::new(StageAction::Report);
    input.stage = Some("implement".into());
    let task = fake.stage_action("T-0141", input).unwrap();
    assert_eq!(task.stage, "agent-review");
}

#[test]
fn reopening_sends_the_task_back_with_the_reason() {
    let fake = FakeDaemon::default();
    let mut input = StageActionInput::new(StageAction::Reopen);
    input.to = Some("implement".into());
    input.reason = Some("Add the error cases.".into());
    let task = fake.stage_action("T-0143", input).unwrap();
    assert_eq!(
        (task.stage.as_str(), task.state),
        ("implement", TaskState::Running)
    );
    let events = accepted(&fake, "T-0143");
    let sent = events
        .iter()
        .rev()
        .find(|e| e.kind == "stage.reopened")
        .unwrap();
    assert_eq!(sent.payload["reason"], "Add the error cases.");
    assert_eq!(sent.payload["to"], "implement");
}

#[test]
fn starting_a_stage_runs_the_task() {
    let fake = FakeDaemon::default();
    let task = fake
        .stage_action("T-0151", StageActionInput::new(StageAction::Start))
        .unwrap();
    assert_eq!(task.state, TaskState::Running);
    assert!(
        fake.stage_action("T-9999", StageActionInput::new(StageAction::Start))
            .is_err()
    );
}

#[test]
fn toggles_change_the_task_lists() {
    let fake = FakeDaemon::default();
    let task = fake
        .set_stage_toggle("T-0150", "workflow", "plan", true)
        .unwrap();
    assert_eq!(task.workflow_stages, ["plan"]);
    let task = fake
        .set_stage_toggle("T-0150", "ci", "implement", true)
        .unwrap();
    assert_eq!(task.ci_stages, Some(vec!["implement".to_string()]));
    let task = fake
        .set_stage_toggle("T-0150", "skip", "security", true)
        .unwrap();
    assert_eq!(task.skip_stages, Some(vec!["security".to_string()]));
    let task = fake
        .set_stage_toggle("T-0150", "skip", "security", false)
        .unwrap();
    assert_eq!(task.skip_stages, Some(Vec::new()));
    assert!(
        fake.set_stage_toggle("T-0150", "nope", "plan", true)
            .is_err()
    );
}

#[test]
fn lists_the_files_of_a_stage_and_fails_for_an_unknown_task() {
    let fake = FakeDaemon::default();
    assert_eq!(fake.list_stage_files("T-0143").unwrap().len(), 6);
    assert!(fake.list_stage_files("T-0141").unwrap().is_empty());
    assert!(fake.list_stage_files("T-9999").is_err());
}

#[test]
fn edits_a_task() {
    let fake = FakeDaemon::default();
    let changes = TaskChanges {
        title: Some("New title".into()),
        priority: Some("low".into()),
        labels: Some(vec!["a".into()]),
        depends_on: Some(vec!["T-0141".into()]),
        description: Some("Notes".into()),
    };
    let task = fake.update_task("T-0131", changes).unwrap();
    assert_eq!(
        (task.title.as_str(), task.priority.as_str()),
        ("New title", "low")
    );
    assert_eq!(task.labels, ["a"]);
    assert_eq!(task.depends_on, ["T-0141"]);
    assert_eq!(task.description, "Notes");
}

#[test]
fn saves_a_template() {
    let fake = FakeDaemon::default();
    let input = TemplateInput {
        name: "My bug".into(),
        title: "Fix: ".into(),
        description: String::new(),
        priority: "high".into(),
        labels: vec!["bug".into()],
    };
    let saved = fake.save_template(input).unwrap();
    assert_eq!(saved.id, "my-bug");
    assert_eq!(fake.list_templates().unwrap().len(), 3);
}

#[test]
fn fans_a_task_out_into_ready_copies_with_a_group_label() {
    let fake = FakeDaemon::default();
    let made = fake
        .fan_out(
            "T-0131",
            FanOutInput {
                count: 2,
                start: false,
            },
        )
        .unwrap();
    let titles: Vec<&str> = made.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Gõ tiếng Việt trong ô tìm kiếm (try 2)",
            "Gõ tiếng Việt trong ô tìm kiếm (try 3)"
        ]
    );
    assert!(
        made.iter()
            .all(|t| t.state == TaskState::Ready && t.workspace.is_none())
    );
    assert!(
        made.iter()
            .all(|t| t.labels.contains(&"tries-t-0131".to_string()))
    );
    let source = fake
        .data()
        .tasks
        .iter()
        .find(|t| t.id == "T-0131")
        .cloned()
        .unwrap();
    assert!(source.labels.contains(&"tries-t-0131".to_string()));
}

#[test]
fn creates_an_issue_and_refreshes_a_pull_request() {
    let fake = FakeDaemon::default();
    let task = fake.create_issue("T-0143").unwrap();
    assert_eq!(task.issue.map(|issue| issue.key), Some("#12".to_string()));
    assert_eq!(fake.refresh_pull_request("T-0143").unwrap().id, "T-0143");
    assert!(fake.refresh_pull_request("T-9999").is_err());
}
