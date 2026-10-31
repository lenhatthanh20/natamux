use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::PullRequestStatus;
use serde_json::json;

use super::*;

fn task(state: TaskState) -> Task {
    Task {
        id: "T-0001".into(),
        title: "Example".into(),
        state,
        branch: Some("natamux/t-0001-example".into()),
        workspace: Some("/work/T-0001".into()),
        ..Task::default()
    }
}

fn asked(message: &str) -> StoredEvent {
    let event: TaskEvent = serde_json::from_value(json!({
        "taskId": "T-0001", "source": "agent", "type": "agent.asked", "at": "",
        "payload": {"message": message}
    }))
    .unwrap();
    StoredEvent {
        seq: 1,
        accepted: true,
        event,
    }
}

fn row(agent: Option<&str>, state: Option<TaskState>) -> ShellRow {
    ShellRow {
        key: "shell:1".into(),
        label: "Terminal 1".into(),
        agent: agent.map(str::to_owned),
        state,
        shell: Some("1".into()),
        plain: agent.is_none(),
    }
}

fn with_stages(stage: &str, stages: Vec<serde_json::Value>) -> Task {
    Task {
        stage: stage.into(),
        stages,
        ..task(TaskState::Running)
    }
}

#[test]
fn a_card_follows_the_open_workspace_unless_a_fold_is_saved() {
    assert_eq!(card_fold_key("T-0001"), "task:T-0001");
    assert!(is_card_open(&[], "T-0001", true));
    assert!(!is_card_open(&[], "T-0001", false));
    let saved = vec![("task:T-0001".to_owned(), true)];
    assert!(is_card_open(&saved, "T-0001", false));
    let folded = vec![("task:T-0001".to_owned(), false)];
    assert!(!is_card_open(&folded, "T-0001", true));
}

#[test]
fn opening_a_task_unfolds_it_and_its_parent() {
    let mut child = task(TaskState::Running);
    child.parent_id = Some("T-0009".into());
    let folds = opened_folds(&[("task:T-0001".to_owned(), false)], &child);
    assert!(is_card_open(&folds, "T-0001", false));
    assert!(is_card_open(&folds, "T-0009", false));
    let folds = opened_folds(&[], &task(TaskState::Running));
    assert_eq!(folds, [("task:T-0001".to_owned(), true)]);
}

#[test]
fn a_waiting_box_has_the_question_and_the_button_of_its_state() {
    let events = [asked("Which branch?")];
    let waiting = waiting_of(&task(TaskState::NeedsInput), Some(&events)).unwrap();
    assert_eq!(waiting.text, "Which branch?");
    assert_eq!(waiting.button, WaitButton::Answer);
    assert_eq!(WaitButton::Answer.label(), "Answer");
    assert_eq!(WaitButton::OpenTerminal.label(), "Open terminal");
    assert_eq!(WaitButton::OpenTask.label(), "Open task");
}

#[test]
fn a_task_that_needs_input_waits_for_the_answer_before_its_events_are_read() {
    let waiting = waiting_of(&task(TaskState::NeedsInput), None).unwrap();
    assert_eq!(waiting.text, "Waiting for your answer");
}

#[test]
fn a_failed_task_opens_its_terminal() {
    let waiting = waiting_of(&task(TaskState::Failed), None).unwrap();
    assert_eq!(waiting.text, "The agent failed.");
    assert_eq!(waiting.button, WaitButton::OpenTerminal);
}

#[test]
fn a_blocked_task_names_what_it_waits_for() {
    let mut blocked = task(TaskState::Blocked);
    assert_eq!(waiting_of(&blocked, None).unwrap().text, "Blocked.");
    blocked.depends_on = vec!["T-0002".into(), "T-0003".into()];
    let waiting = waiting_of(&blocked, None).unwrap();
    assert_eq!(waiting.text, "Waits for T-0002, T-0003.");
    assert_eq!(waiting.button, WaitButton::OpenTask);
}

#[test]
fn a_task_that_does_not_need_the_user_has_no_waiting_box() {
    for state in [TaskState::Running, TaskState::Review, TaskState::Done] {
        assert_eq!(waiting_of(&task(state), None), None);
    }
}

#[test]
fn the_stage_chip_has_the_step_and_the_stage() {
    let two = with_stages("implement", vec![json!("plan"), json!("implement")]);
    let chip = stage_chip(&two).unwrap();
    assert_eq!(chip.step.as_deref(), Some("2/2"));
    assert_eq!(chip.stage, stage_name("implement"));
    assert_eq!(card_stage(&two), Some(stage_name("implement")));
}

#[test]
fn a_task_with_one_stage_has_no_step() {
    let one = with_stages("implement", vec![json!("implement")]);
    assert_eq!(stage_chip(&one).unwrap().step, None);
}

#[test]
fn a_parallel_group_joins_its_stages() {
    let group = json!({"id": "work", "parallel": ["build", "docs"]});
    let task = with_stages("build", vec![json!("plan"), group]);
    let chip = stage_chip(&task).unwrap();
    assert_eq!(chip.step.as_deref(), Some("2/2"));
    assert_eq!(
        chip.stage,
        format!("{} ∥ {}", stage_name("build"), stage_name("docs"))
    );
}

#[test]
fn a_draft_and_a_finished_task_have_no_stage() {
    for state in [TaskState::Draft, TaskState::Done, TaskState::Cancelled] {
        let mut each = with_stages("implement", vec![json!("implement")]);
        each.state = state;
        assert_eq!(card_stage(&each), None);
        assert_eq!(stage_chip(&each), None);
    }
}

#[test]
fn a_card_has_one_avatar_per_agent_terminal() {
    let rows = [
        row(Some("claude-code"), Some(TaskState::Running)),
        row(None, None),
        row(Some("codex"), None),
    ];
    let agents = card_agents(&task(TaskState::Running), &rows);
    assert_eq!(
        agents,
        [
            AgentAvatar {
                agent: "claude-code".into(),
                state: Some(TaskState::Running)
            },
            AgentAvatar {
                agent: "codex".into(),
                state: None
            },
        ]
    );
}

#[test]
fn a_card_without_agent_terminals_shows_the_agent_of_the_task() {
    let mut review = task(TaskState::Review);
    review.agent = Some("claude-code".into());
    let rows = [row(None, None)];
    assert_eq!(
        card_agents(&review, &rows),
        [AgentAvatar {
            agent: "claude-code".into(),
            state: Some(TaskState::Review)
        }]
    );
    review.agent = None;
    assert!(card_agents(&review, &rows).is_empty());
}

#[test]
fn more_than_three_agents_show_a_plus_count() {
    assert_eq!(more_agents(3), None);
    assert_eq!(more_agents(5).as_deref(), Some("+2"));
}

#[test]
fn the_pull_request_chip_shows_the_checks() {
    let mut review = task(TaskState::Review);
    assert_eq!(pr_chip(&review), None);
    review.pr = Some("https://github.com/o/r/pull/81234".into());
    assert_eq!(
        pr_chip(&review),
        Some(PrChip {
            text: "#81234".into(),
            tone: None
        })
    );
    let status = |checks: &str| {
        Some(PullRequestStatus {
            state: "open".into(),
            checks: checks.into(),
        })
    };
    let cases = [
        ("passing", "#81234 ✓", Some(TaskState::Done)),
        ("failing", "#81234 ✕", Some(TaskState::Failed)),
        ("pending", "#81234 ●", Some(TaskState::Running)),
        ("none", "#81234", None),
    ];
    for (checks, text, tone) in cases {
        review.pr_status = status(checks);
        let chip = pr_chip(&review).unwrap();
        assert_eq!((chip.text.as_str(), chip.tone), (text, tone));
    }
}
