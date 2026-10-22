use super::*;
use natamux_proto::workspaces::workspace::PushStatus;
use serde_json::{Value, json};

const PR: &str = "https://github.com/o/r/pull/310";
const PUSHED: PushStatus = PushStatus::Tracked {
    unpushed: 0,
    unpulled: 0,
};

fn make(state: &str, pr: Option<&str>, pr_status: Value) -> Task {
    serde_json::from_value(json!({
        "id": "T-0058", "title": "t", "updatedAt": "", "state": state,
        "pr": pr, "prStatus": pr_status, "branch": "natamux/t-0058-icons",
    }))
    .expect("a task")
}

fn status(state: &str) -> Value {
    json!({"state": state, "checks": "passing"})
}

fn done() -> Task {
    make("done", None, Value::Null)
}

fn merged() -> Task {
    make("done", Some(PR), status("merged"))
}

fn open() -> Task {
    make("done", Some(PR), status("open"))
}

fn cancelled() -> Task {
    make("cancelled", None, Value::Null)
}

fn facts(uncommitted: u64, ahead: u64, push: Option<PushStatus>) -> GitFacts {
    GitFacts {
        uncommitted: Some(uncommitted),
        graph: Some(GitGraph {
            base: Some("main".into()),
            ahead: Some(ahead),
            push,
            ..GitGraph::default()
        }),
    }
}

fn clean(ahead: u64) -> GitFacts {
    facts(0, ahead, Some(PUSHED))
}

fn banner_text(task: &Task, facts: &GitFacts) -> (Option<String>, Option<FinishedAction>) {
    let banner = finished_banner(task, facts);
    (banner.text, banner.primary)
}

fn is(text: &str, primary: FinishedAction) -> (Option<String>, Option<FinishedAction>) {
    (Some(text.into()), Some(primary))
}

#[test]
fn is_true_for_done_and_cancelled_only() {
    assert!(is_finished_state(TaskState::Done));
    assert!(is_finished_state(TaskState::Cancelled));
    assert!(!is_finished_state(TaskState::Review));
}

#[test]
fn asks_for_a_commit_first_and_says_why_the_workspace_cannot_go_yet() {
    assert_eq!(
        finished_banner(&merged(), &facts(3, 0, Some(PUSHED))),
        FinishedBannerText {
            title: "Task done.".into(),
            text: Some("3 files are not committed.".into()),
            primary: Some(FinishedAction::Commit),
            remove_reason: Some("Commit or discard them first".into()),
        }
    );
    assert_eq!(
        finished_banner(&cancelled(), &facts(1, 0, Some(PUSHED))).text,
        Some("1 file is not committed.".into())
    );
}

#[test]
fn asks_for_a_push_of_commits_that_origin_does_not_have() {
    let push = PushStatus::Tracked {
        unpushed: 2,
        unpulled: 0,
    };
    assert_eq!(
        finished_banner(&done(), &facts(0, 2, Some(push))),
        FinishedBannerText {
            title: "Task done.".into(),
            text: Some("2 commits are not pushed.".into()),
            primary: Some(FinishedAction::Push),
            remove_reason: None,
        }
    );
}

#[test]
fn asks_for_a_pull_request_when_the_branch_has_work_and_none() {
    assert_eq!(
        banner_text(&done(), &clean(2)),
        is(
            "Its branch has no pull request.",
            FinishedAction::CreatePullRequest
        )
    );
}

#[test]
fn points_to_a_pull_request_that_is_still_open() {
    assert_eq!(
        banner_text(&open(), &clean(2)),
        is(
            "Pull request #310 is still open.",
            FinishedAction::OpenPullRequest
        )
    );
}

#[test]
fn offers_to_remove_the_workspace_when_nothing_is_left() {
    assert_eq!(
        finished_banner(&merged(), &clean(2)),
        FinishedBannerText {
            title: "Task done.".into(),
            text: Some("Pull request #310 is merged. Nothing is left in the workspace.".into()),
            primary: Some(FinishedAction::Remove),
            remove_reason: None,
        }
    );
    assert_eq!(
        banner_text(&done(), &clean(0)),
        is("Nothing is left in the workspace.", FinishedAction::Remove)
    );
    let closed = make(
        "done",
        Some(PR),
        json!({"state": "closed", "checks": "none"}),
    );
    assert_eq!(
        banner_text(&closed, &clean(2)),
        is(
            "Pull request #310 was closed without a merge.",
            FinishedAction::Remove
        )
    );
}

#[test]
fn offers_to_remove_the_workspace_of_a_cancelled_task_whatever_git_says() {
    assert_eq!(
        finished_banner(&cancelled(), &facts(0, 2, None)),
        FinishedBannerText {
            title: "Task cancelled.".into(),
            text: Some("Its commits and branch are kept.".into()),
            primary: Some(FinishedAction::Remove),
            remove_reason: None,
        }
    );
    assert_eq!(
        finished_banner(&cancelled(), &clean(0)).text,
        Some("Nothing is left in the workspace.".into())
    );
}

#[test]
fn says_only_the_outcome_while_git_is_read() {
    assert_eq!(
        finished_banner(&merged(), &GitFacts::default()),
        FinishedBannerText {
            title: "Task done.".into(),
            text: None,
            primary: None,
            remove_reason: None,
        }
    );
    let no_graph = GitFacts {
        uncommitted: Some(0),
        graph: None,
    };
    assert_eq!(finished_banner(&merged(), &no_graph).primary, None);
}

fn row_of(rows: &[WrapUpRow], id: WrapUpRowId) -> WrapUpRow {
    rows.iter()
        .find(|row| row.id == id)
        .cloned()
        .expect("a row with that id")
}

fn plain(id: WrapUpRowId, tone: RowTone, text: &str) -> WrapUpRow {
    WrapUpRow {
        id,
        tone,
        text: text.into(),
        detail: None,
        action: None,
    }
}

#[test]
fn ticks_every_row_of_a_merged_task_with_nothing_left() {
    assert_eq!(
        wrap_up_rows(&merged(), &clean(0), &[]),
        vec![
            plain(
                WrapUpRowId::Changes,
                RowTone::Ok,
                "All changes are committed"
            ),
            plain(WrapUpRowId::Push, RowTone::Ok, "Pushed to origin"),
            WrapUpRow {
                detail: Some("Checks pass".into()),
                action: Some(FinishedAction::OpenPullRequest),
                ..plain(
                    WrapUpRowId::PullRequest,
                    RowTone::Ok,
                    "Pull request #310 merged into main"
                )
            },
            plain(
                WrapUpRowId::Branch,
                RowTone::Ok,
                "The branch is merged. Removing the workspace deletes it."
            ),
        ]
    );
}

#[test]
fn lists_up_to_three_files_that_are_not_committed() {
    let paths: Vec<String> = ["a.ts", "b.ts", "c.ts", "d.ts", "e.ts"]
        .map(String::from)
        .to_vec();
    let rows = wrap_up_rows(&done(), &facts(5, 0, Some(PUSHED)), &paths);
    assert_eq!(
        row_of(&rows, WrapUpRowId::Changes),
        WrapUpRow {
            detail: Some("a.ts, b.ts, c.ts and 2 more".into()),
            action: Some(FinishedAction::ViewChanges),
            ..plain(
                WrapUpRowId::Changes,
                RowTone::Warn,
                "5 files are not committed"
            )
        }
    );
}

#[test]
fn asks_for_a_push_or_a_publish_of_a_branch_origin_does_not_have() {
    let push_of = |facts: GitFacts| row_of(&wrap_up_rows(&done(), &facts, &[]), WrapUpRowId::Push);
    let one = PushStatus::Tracked {
        unpushed: 1,
        unpulled: 0,
    };
    assert_eq!(
        push_of(facts(0, 0, Some(one))),
        WrapUpRow {
            action: Some(FinishedAction::Push),
            ..plain(WrapUpRowId::Push, RowTone::Warn, "1 commit is not pushed")
        }
    );
    let fresh = push_of(facts(0, 2, Some(PushStatus::New { unpushed: 2 })));
    assert_eq!(fresh.tone, RowTone::Warn);
    assert_eq!(fresh.text, "The branch is not on origin yet");
    assert_eq!(
        push_of(facts(0, 0, Some(PushStatus::None))),
        plain(
            WrapUpRowId::Push,
            RowTone::Muted,
            "This project has no remote named origin"
        )
    );
}

#[test]
fn says_where_the_pull_request_stands() {
    let pr_of = |task: &Task, ahead| {
        row_of(
            &wrap_up_rows(task, &clean(ahead), &[]),
            WrapUpRowId::PullRequest,
        )
    };
    assert_eq!(
        pr_of(&open(), 2),
        WrapUpRow {
            detail: Some("Checks pass".into()),
            action: Some(FinishedAction::OpenPullRequest),
            ..plain(
                WrapUpRowId::PullRequest,
                RowTone::Info,
                "Pull request #310 is open"
            )
        }
    );
    assert_eq!(
        pr_of(&done(), 2),
        WrapUpRow {
            action: Some(FinishedAction::CreatePullRequest),
            ..plain(WrapUpRowId::PullRequest, RowTone::Muted, "No pull request")
        }
    );
    assert_eq!(
        pr_of(&cancelled(), 2),
        plain(WrapUpRowId::PullRequest, RowTone::Muted, "No pull request")
    );
}

#[test]
fn says_the_branch_stays_when_it_has_work_that_is_not_merged() {
    let rows = wrap_up_rows(&cancelled(), &clean(2), &[]);
    assert_eq!(
        row_of(&rows, WrapUpRowId::Branch),
        WrapUpRow {
            detail: Some("It stays after the workspace is removed".into()),
            ..plain(
                WrapUpRowId::Branch,
                RowTone::Info,
                "The branch has 2 commits that are not merged"
            )
        }
    );
}

#[test]
fn marks_what_git_has_not_told_yet() {
    let rows = wrap_up_rows(&merged(), &GitFacts::default(), &[]);
    let seen: Vec<(WrapUpRowId, RowTone, &str)> = rows
        .iter()
        .map(|row| (row.id, row.tone, row.text.as_str()))
        .collect();
    assert_eq!(
        seen,
        vec![
            (WrapUpRowId::Changes, RowTone::Muted, "Reading the changes…"),
            (WrapUpRowId::Push, RowTone::Muted, "Reading the branch…"),
            (
                WrapUpRowId::PullRequest,
                RowTone::Ok,
                "Pull request #310 merged"
            ),
            (WrapUpRowId::Branch, RowTone::Muted, "Reading the branch…"),
        ]
    );
}

fn stored(seq: u64, kind: &str, at: &str, accepted: bool) -> StoredEvent {
    serde_json::from_value(json!({
        "seq": seq, "accepted": accepted,
        "event": {"taskId": "T-0058", "source": "human", "type": kind, "at": at, "payload": {}},
    }))
    .expect("an event")
}

#[test]
fn takes_the_last_accepted_event_that_moved_the_task() {
    let events = [
        stored(1, "task.created", "2026-10-14T09:00:00.000Z", true),
        stored(2, "stage.approved", "2026-10-14T17:07:00.000Z", true),
        stored(3, "agent.finished", "2026-10-14T17:08:00.000Z", false),
        stored(4, "workspace.removed", "2026-10-14T17:09:00.000Z", true),
    ];
    assert_eq!(
        finished_at(&events).as_deref(),
        Some("2026-10-14T17:07:00.000Z")
    );
    assert_eq!(finished_at(&[]), None);
}

#[test]
fn says_how_the_task_ended() {
    assert_eq!(finished_text(&done()), "T-0058 is done.");
    assert_eq!(finished_text(&cancelled()), "T-0058 was cancelled.");
}
