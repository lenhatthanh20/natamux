use natamux_proto::stages::schema::Gate;

use super::*;

fn entry(status: StageStatus, state: Option<TaskState>) -> StageProgress {
    StageProgress {
        stage: "plan".into(),
        name: "Plan".into(),
        kind: StageKind::Agent,
        gate: Gate::Manual,
        output: None,
        group: None,
        path: None,
        because: None,
        status,
        state,
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

fn current(state: TaskState) -> StageProgress {
    entry(StageStatus::Current, Some(state))
}

fn asked() -> WaitReason {
    WaitReason::Asked {
        question: Some("Before or after tax?".into()),
    }
}

#[test]
fn puts_a_wide_approve_anyway_first_when_the_checks_failed() {
    let mut waits = current(TaskState::NeedsInput);
    waits.reasons = vec!["plan.md has no Tests section".into()];
    let facts = card_facts(
        &waits,
        StageKind::Agent,
        false,
        &WaitReason::Stopped,
        true,
        true,
    );
    assert_eq!(facts.buttons[0], CardButton::Approve { anyway: true });
    assert_eq!(facts.notices, vec![CardNotice::Reasons { failed: false }]);
}

#[test]
fn shows_why_the_agent_failed_in_red_once_with_start_agent() {
    let mut failed = current(TaskState::Failed);
    failed.reasons = vec!["The Plan agent stopped.".into()];
    let reason = WaitReason::Failed {
        reason: Some("The Plan agent stopped.".into()),
    };
    let facts = card_facts(&failed, StageKind::Agent, false, &reason, true, true);
    assert_eq!(facts.notices, vec![CardNotice::Reasons { failed: true }]);
    assert!(facts.buttons.contains(&CardButton::PickAgent));
    assert!(facts.buttons.contains(&CardButton::StartAgent));
    assert!(
        !facts
            .buttons
            .iter()
            .any(|b| matches!(b, CardButton::Approve { .. }))
    );
}

#[test]
fn says_a_failed_stage_failed_when_its_agent_gave_no_reason() {
    let failed = current(TaskState::Failed);
    assert_eq!(
        card_notices(&failed, &WaitReason::Stopped),
        vec![CardNotice::Wait(WaitReason::Failed { reason: None })]
    );
}

#[test]
fn shows_the_question_of_a_waiting_agent_with_answer_in_terminal_and_resume() {
    let facts = card_facts(
        &current(TaskState::NeedsInput),
        StageKind::Agent,
        false,
        &asked(),
        true,
        true,
    );
    assert_eq!(
        facts.buttons,
        vec![
            CardButton::Approve { anyway: true },
            CardButton::AnswerInTerminal,
            CardButton::Resume,
            CardButton::PickAgent,
            CardButton::StartAgent,
            CardButton::MarkAsDone,
            CardButton::Skip,
            CardButton::SendBack,
        ]
    );
    assert_eq!(facts.notices, vec![CardNotice::Wait(asked())]);
}

#[test]
fn offers_answer_in_terminal_only_for_a_question_with_a_terminal_beside() {
    let waits = current(TaskState::NeedsInput);
    let answers = |asked: bool, beside: bool| {
        card_buttons(&waits, StageKind::Agent, asked, beside)
            .contains(&CardButton::AnswerInTerminal)
    };
    assert!(answers(true, true));
    assert!(!answers(true, false));
    assert!(!answers(false, true));
}

#[test]
fn shows_no_wait_text_in_review_nor_beside_the_reasons_of_failed_checks() {
    assert_eq!(
        card_notices(&current(TaskState::Review), &asked()),
        Vec::new()
    );
    let mut fixing = current(TaskState::Running);
    fixing.reasons = vec!["2 tests fail".into()];
    assert_eq!(card_notices(&fixing, &asked()), Vec::new());
}

#[test]
fn skips_the_stage_and_marks_a_running_one_as_done() {
    let buttons = card_buttons(&current(TaskState::Running), StageKind::Agent, false, true);
    assert_eq!(
        buttons,
        vec![
            CardButton::MarkAsDone,
            CardButton::Skip,
            CardButton::SendBack
        ]
    );
}

#[test]
fn shows_one_line_with_the_finish_rule_for_a_stage_not_reached_yet() {
    let upcoming = entry(StageStatus::Upcoming, None);
    let facts = card_facts(&upcoming, StageKind::Review, false, &asked(), true, true);
    assert_eq!(facts.chip, CardChip::NotStarted);
    assert!(facts.buttons.is_empty() && facts.notices.is_empty());
    // A review stage that is current but not started says what finishes it, with no Start.
    let waiting = card_buttons(&current(TaskState::Ready), StageKind::Review, false, true);
    assert_eq!(waiting[0], CardButton::FinishedWhen);
    let starts = card_buttons(&current(TaskState::Ready), StageKind::Agent, false, true);
    assert_eq!(starts[0], CardButton::Start);
}

#[test]
fn shows_a_marked_stage_and_takes_the_mark_off() {
    let upcoming = entry(StageStatus::Upcoming, None);
    assert_eq!(card_chip(&upcoming, true), CardChip::WillBeSkipped);
    assert_eq!(card_chip(&upcoming, false), CardChip::NotStarted);
    assert_eq!(
        card_chip(&current(TaskState::Review), true),
        CardChip::State(TaskState::Review)
    );
}

#[test]
fn offers_no_actions_on_a_task_that_is_over() {
    let facts = card_facts(
        &current(TaskState::Review),
        StageKind::Review,
        false,
        &asked(),
        true,
        false,
    );
    assert!(facts.buttons.is_empty());
}

#[test]
fn ends_only_the_done_stages_of_a_done_task_with_the_task() {
    let task = Task {
        state: TaskState::Done,
        updated_at: "2026-10-22T10:00:00.000Z".into(),
        ..Task::default()
    };
    let mut progress = vec![
        entry(StageStatus::Done, None),
        entry(StageStatus::Skipped, None),
        entry(StageStatus::NotTaken, None),
        entry(StageStatus::Upcoming, None),
    ];
    end_with_the_task(&mut progress, &task);
    let ends: Vec<Option<&str>> = progress.iter().map(|e| e.ended_at.as_deref()).collect();
    assert_eq!(
        ends,
        vec![Some("2026-10-22T10:00:00.000Z"), None, None, None]
    );
    let mut open = vec![entry(StageStatus::Done, None)];
    end_with_the_task(&mut open, &Task::default());
    assert_eq!(open[0].ended_at, None);
}

#[test]
fn links_every_address_in_a_reason_wherever_it_is_without_the_punctuation_after_it() {
    let parts = reason_parts("See https://example.com/a. And (https://x.dev/b), done");
    assert_eq!(
        parts,
        vec![
            ReasonPart::Text("See ".into()),
            ReasonPart::Link {
                url: "https://example.com/a".into(),
                shown: "https://example.com/a".into(),
            },
            ReasonPart::Text(". And (".into()),
            ReasonPart::Link {
                url: "https://x.dev/b".into(),
                shown: "https://x.dev/b".into(),
            },
            ReasonPart::Text("), done".into()),
        ]
    );
    assert_eq!(
        reason_parts("no address"),
        vec![ReasonPart::Text("no address".into())]
    );
}

#[test]
fn lists_the_failed_runs_with_short_links_when_the_stage_waits_for_you() {
    let run = "https://github.com/o/r/actions/runs/123/job/4";
    assert_eq!(
        reason_parts(&format!("CI failed: {run}")),
        vec![
            ReasonPart::Text("CI failed: ".into()),
            ReasonPart::Link {
                url: run.into(),
                shown: "view run".into(),
            },
        ]
    );
}
