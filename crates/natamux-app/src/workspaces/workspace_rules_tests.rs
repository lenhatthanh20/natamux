use super::*;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::Task;
use serde_json::json;

#[test]
fn needs_a_title_and_a_reference_for_a_source_other_than_manual() {
    assert_eq!(
        create_blocked_reason(" ", "manual", "", None, None),
        Some("Enter a title")
    );
    assert_eq!(create_blocked_reason("Fix", "manual", "", None, None), None);
    assert_eq!(
        create_blocked_reason("Fix", "github-issue", " ", None, None),
        Some("Enter the reference of the context")
    );
    assert_eq!(
        create_blocked_reason("Fix", "github-issue", "#4", None, None),
        None
    );
}

#[test]
fn needs_a_skill_a_reference_and_an_agent_for_a_skill_picked_in_the_dialog() {
    let reason = |r, s, a| create_blocked_reason("Fix", "skill", r, s, a);
    assert_eq!(
        reason("", Some(""), Some("claude-code")),
        Some("Pick a skill")
    );
    assert_eq!(
        reason(" ", Some("jira-ticket"), Some("claude-code")),
        Some("Enter the reference of the context")
    );
    assert_eq!(
        reason("ABC-1", Some("jira-ticket"), Some("")),
        Some("Pick an agent to run the skill")
    );
    assert_eq!(reason("ABC-1", Some("jira-ticket"), Some("codex")), None);
    // A skill from the project settings needs no pick.
    assert_eq!(reason("ABC-1", None, Some("codex")), None);
}

#[test]
fn gives_one_reason_at_a_time_title_skill_reference_then_agent() {
    assert_eq!(
        create_blocked_reason(" ", "skill", "", Some(""), Some("")),
        Some("Enter a title")
    );
    assert_eq!(
        create_blocked_reason("Fix", "skill", "", Some(""), Some("")),
        Some("Pick a skill")
    );
    assert_eq!(
        create_blocked_reason("Fix", "skill", "  ", Some("jira-ticket"), Some("")),
        Some("Enter the reference of the context")
    );
}

#[test]
fn needs_no_agent_for_my_notes_or_for_a_skill_from_the_settings() {
    assert_eq!(
        create_blocked_reason("Fix", "manual", "", None, Some("")),
        None
    );
    assert_eq!(
        create_blocked_reason("Fix", "skill", "ABC-1", None, Some("")),
        None
    );
}

#[test]
fn opens_the_terminal_while_the_agent_works_or_waits_whatever_was_used_last() {
    assert_eq!(
        tab_to_open(TaskState::Running, Some(WorkspaceTab::Changes)),
        WorkspaceTab::Terminal
    );
    assert_eq!(
        tab_to_open(TaskState::NeedsInput, Some(WorkspaceTab::Docs)),
        WorkspaceTab::Terminal
    );
}

#[test]
fn opens_the_last_tab_of_that_workspace_and_the_terminal_for_a_new_one() {
    assert_eq!(
        tab_to_open(TaskState::Review, Some(WorkspaceTab::Changes)),
        WorkspaceTab::Changes
    );
    assert_eq!(
        tab_to_open(TaskState::Ready, Some(WorkspaceTab::Docs)),
        WorkspaceTab::Docs
    );
    assert_eq!(tab_to_open(TaskState::Ready, None), WorkspaceTab::Terminal);
}

fn tracked(unpushed: u64, unpulled: u64) -> PushStatus {
    PushStatus::Tracked { unpushed, unpulled }
}

fn button(label: &str, reason: Option<&str>, tip: Option<&str>) -> GitButton {
    GitButton {
        label: label.into(),
        reason: reason.map(Into::into),
        tip: tip.map(Into::into),
    }
}

fn asked(seq: u64, payload: serde_json::Value) -> StoredEvent {
    serde_json::from_value(json!({
        "seq": seq,
        "accepted": true,
        "event": {"taskId": "T-0001", "source": "agent", "type": "agent.asked", "at": "", "payload": payload},
    }))
    .expect("an event")
}

fn task(stage: &str, lanes: serde_json::Value) -> Task {
    serde_json::from_value(
        json!({"id": "T-0001", "title": "t", "updatedAt": "", "stage": stage, "lanes": lanes}),
    )
    .expect("a task")
}

#[test]
fn shows_the_commits_that_origin_does_not_have_on_the_label() {
    assert_eq!(
        push_button(Some("b"), Some(&tracked(3, 0)), None),
        button("Push ↑3", None, Some("Push 3 commits to origin/b"))
    );
}

#[test]
fn offers_to_publish_a_branch_that_origin_does_not_have() {
    let one = PushStatus::New { unpushed: 1 };
    assert_eq!(
        push_button(Some("b"), Some(&one), None),
        button(
            "Publish branch",
            None,
            Some("Publish b to origin with 1 commit")
        )
    );
    let three = PushStatus::New { unpushed: 3 };
    assert_eq!(
        push_button(Some("b"), Some(&three), None).tip.as_deref(),
        Some("Publish b to origin with 3 commits")
    );
}

#[test]
fn shows_the_full_count_however_large() {
    assert_eq!(
        push_button(Some("b"), Some(&tracked(120, 0)), None).label,
        "Push ↑120"
    );
}

#[test]
fn works_with_a_plain_label_while_the_status_is_unknown() {
    assert_eq!(
        push_button(Some("b"), None, None),
        button("Push", None, Some("Push b to origin"))
    );
}

#[test]
fn says_why_there_is_nothing_to_push() {
    assert_eq!(
        push_button(None, None, None),
        button("Push", Some("This task has no branch to push"), None)
    );
    // A task without a branch wins over any status.
    let no_branch = Some("This task has no branch to push");
    assert_eq!(
        push_button(None, Some(&tracked(2, 0)), None)
            .reason
            .as_deref(),
        no_branch
    );
    assert_eq!(
        push_button(None, Some(&tracked(1, 2)), None)
            .reason
            .as_deref(),
        no_branch
    );
    assert_eq!(
        push_button(Some("b"), Some(&PushStatus::None), None),
        button(
            "Push",
            Some("This project has no remote named origin"),
            None
        )
    );
    assert_eq!(
        push_button(Some("b"), Some(&tracked(0, 0)), None),
        button(
            "Push",
            Some("Nothing to push: up to date with origin/b"),
            None
        )
    );
    assert_eq!(
        push_button(Some("b"), Some(&PushStatus::New { unpushed: 0 }), None),
        button(
            "Publish branch",
            Some("Nothing to publish: every commit is on origin already"),
            None
        )
    );
}

#[test]
fn is_off_while_origin_has_commits_that_the_branch_does_not_have() {
    assert_eq!(
        push_button(Some("b"), Some(&tracked(2, 1)), None),
        button(
            "Push ↑2 ↓1",
            Some("origin/b has 1 commit that this branch does not have. Pull or rebase first."),
            None
        )
    );
    assert_eq!(
        push_button(Some("b"), Some(&tracked(0, 2)), None),
        button(
            "Push ↓2",
            Some("origin/b has 2 commits that this branch does not have. Pull or rebase first."),
            None
        )
    );
}

#[test]
fn asks_for_a_commit_first_when_files_are_not_committed_and_nothing_is_unpushed() {
    assert_eq!(
        push_button(Some("b"), Some(&tracked(0, 0)), Some(2))
            .reason
            .as_deref(),
        Some("Commit your 2 changed files first")
    );
    assert_eq!(
        push_button(Some("b"), Some(&PushStatus::New { unpushed: 0 }), Some(1)),
        button(
            "Publish branch",
            Some("Commit your changed file first"),
            None
        )
    );
    // With no origin, Commit does not take Push's place: there is no count to compare.
    assert_eq!(
        push_button(Some("b"), Some(&PushStatus::None), Some(2)),
        button(
            "Push",
            Some("This project has no remote named origin"),
            None
        )
    );
    // Nothing to push and files to commit: committing comes before a pull or rebase.
    assert_eq!(
        push_button(Some("b"), Some(&tracked(0, 2)), Some(1)),
        button("Push", Some("Commit your changed file first"), None)
    );
    assert_eq!(
        push_button(Some("b"), Some(&tracked(1, 0)), Some(3)),
        button("Push ↑1", None, Some("Push 1 commit to origin/b"))
    );
    assert!(needs_commit(Some(&tracked(0, 0)), Some(2)));
    assert!(needs_commit(Some(&tracked(0, 2)), Some(2)));
    assert!(needs_commit(
        Some(&PushStatus::New { unpushed: 0 }),
        Some(2)
    ));
    assert!(!needs_commit(Some(&tracked(0, 0)), Some(0)));
    assert!(!needs_commit(Some(&tracked(1, 0)), Some(2)));
    assert!(!needs_commit(Some(&tracked(0, 0)), None));
    assert!(!needs_commit(None, Some(2)));
    assert!(!needs_commit(Some(&PushStatus::None), Some(2)));
}

#[test]
fn says_create_pr() {
    assert_eq!(
        pull_request_button(Some("b"), Some(1), Some("main"), None).label,
        "Create PR"
    );
}

#[test]
fn asks_for_a_push_first_when_there_is_nothing_ahead_and_for_a_branch() {
    let reason = |b, a, base, u| pull_request_button(b, a, base, u).reason;
    assert_eq!(
        reason(Some("b"), Some(0), Some("main"), None).as_deref(),
        Some("Push first: 0 commits ahead of main")
    );
    assert_eq!(
        reason(None, None, None, None).as_deref(),
        Some("This task has no branch yet")
    );
    assert_eq!(reason(Some("b"), None, None, None), None);
    assert_eq!(
        reason(Some("b"), Some(0), Some("main"), Some(4)).as_deref(),
        Some("Commit your 4 changed files first")
    );
}

#[test]
fn offers_a_push_after_a_commit_and_a_pull_request_after_a_push() {
    assert_eq!(next_git_step(GitStep::Commit, false), Some(GitStep::Push));
    assert_eq!(
        next_git_step(GitStep::Push, false),
        Some(GitStep::PullRequest)
    );
    assert_eq!(next_git_step(GitStep::Push, true), None);
    assert_eq!(next_git_step(GitStep::PullRequest, false), None);
}

#[test]
fn reads_the_message_of_the_newest_question() {
    let events = [
        asked(1, json!({"message": "Old?"})),
        asked(2, json!({"message": "  Which\n runner? "})),
    ];
    assert_eq!(latest_question(&events).as_deref(), Some("Which runner?"));
}

#[test]
fn skips_questions_without_a_message_and_other_events() {
    let events = [asked(1, json!({"message": "Real?"})), asked(2, json!({}))];
    assert_eq!(latest_question(&events).as_deref(), Some("Real?"));
    assert_eq!(latest_question(&[]), None);
}

#[test]
fn names_the_stages_of_a_group_that_are_in_review_else_the_task_stage() {
    assert_eq!(review_stages(&task("plan", json!({}))), vec!["plan"]);
    let group = json!({"test-spec": "review", "implement": "running"});
    assert_eq!(review_stages(&task("build", group)), vec!["test-spec"]);
    let running = json!({"a": "running", "b": "running"});
    assert_eq!(review_stages(&task("build", running)), vec!["build"]);
}

#[test]
fn assumes_a_question_while_the_events_load() {
    use natamux_proto::events::task_event::{WaitReason, wait_reason};
    assert_eq!(wait_reason(None), WaitReason::Asked { question: None });
}

#[test]
fn reads_the_question_of_the_newest_ask() {
    use natamux_proto::events::task_event::{WaitReason, wait_reason};
    let events = [asked(1, json!({"message": "Which runner?"}))];
    assert_eq!(
        wait_reason(Some(&events)),
        WaitReason::Asked {
            question: Some("Which runner?".into())
        }
    );
}
