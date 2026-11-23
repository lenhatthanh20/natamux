use natamux_core::security::CiMode;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::stages::schema::{Gate, StageKind};
use serde_json::json;

use super::*;

fn target(commit: &str) -> CiTarget {
    CiTarget {
        branch: "natamux/t-1".into(),
        commit: commit.into(),
        tree: format!("{commit}-tree"),
        remote: "https://example.com/o/r.git".into(),
        mode: CiMode::Snapshot,
    }
}

fn approved(seq: u64, stage: &str, commit: &str) -> StoredEvent {
    serde_json::from_value(json!({
        "seq": seq,
        "accepted": true,
        "event": {"taskId": "T-1", "source": "system", "type": "stage.ci-approved", "at": "x",
            "payload": {"stage": stage, "mode": "snapshot", "remote": "https://example.com/o/r.git",
                "branch": "natamux-ci/natamux/t-1", "commit": commit,
                "tree": format!("{commit}-tree"), "files": [], "newCommits": 0,
                "approver": "auto"}}
    }))
    .unwrap()
}

fn event(seq: u64, kind: &str, stage: Option<&str>) -> StoredEvent {
    let payload = stage.map_or(json!({}), |stage| json!({ "stage": stage }));
    serde_json::from_value(json!({
        "seq": seq,
        "accepted": true,
        "event": {"taskId": "T-1", "source": "system", "type": kind, "at": "x", "payload": payload}
    }))
    .unwrap()
}

#[test]
fn finds_the_stages_whose_ci_check_started_and_has_no_result() {
    let events = [
        event(1, "stage.ci-started", Some("implement")),
        event(2, "stage.ci-started", Some("testing")),
        event(3, "stage.check-failed", Some("implement")),
        event(4, "stage.ci-started", Some("implement")),
        event(5, "stage.ci-started", None),
        event(6, "stage.checked", Some("testing")),
    ];
    assert_eq!(
        stages_waiting_for_ci(&events),
        vec!["implement".to_string()]
    );
    assert_eq!(
        stages_waiting_for_ci(&events[..2]),
        vec!["implement".to_string(), "testing".to_string()]
    );
    assert!(stages_waiting_for_ci(&[]).is_empty());
}

#[test]
fn a_restarted_stage_stops_waiting() {
    let events = [
        event(1, "stage.ci-started", Some("implement")),
        event(2, "stage.started", Some("implement")),
    ];
    assert!(stages_waiting_for_ci(&events).is_empty());
}

#[test]
fn answers_as_node_does_where_gh_is_missing() {
    let result = NoCi.check(std::path::Path::new("/w"), &target("abc"));
    assert_eq!(
        result,
        CiResult {
            passed: false,
            reasons: vec![
                "Could not read the CI runs: GitHub CLI failed: gh is not installed or not on the PATH"
                    .into()
            ],
            passes: vec![],
            final_: true,
            logs: None,
        }
    );
    assert_eq!(ci_branch_of("a/b"), "natamux-ci/a/b");
}

#[test]
fn writes_the_reason_of_a_send_back_for_a_ci_failure() {
    let stage = Stage::new("implement", StageKind::Agent, Gate::Auto);
    let reason = ci_reason(
        &stage,
        &["build failed".to_string()],
        &["\u{1b}[31mboom\u{1b}[0m \n".to_string()],
    );
    assert_eq!(
        reason,
        "CI failed on Implement:\n- build failed\n\nboom\nRead the failed log with `gh run view <run id> --log-failed`; the id ends the run url."
    );
}

#[test]
fn picks_the_latest_approval_of_the_stage() {
    let events = [
        approved(1, "implement", "aaa"),
        approved(2, "testing", "bbb"),
        event(3, "stage.ci-started", Some("implement")),
        approved(4, "implement", "ccc"),
        event(5, "stage.ci-started", Some("implement")),
    ];
    let found = ci_approval_of(&events, "implement").unwrap();
    assert_eq!(found.seq, 4);
    assert_eq!(found.target, target("ccc"));
    assert_eq!(ci_approval_of(&events, "testing").unwrap().seq, 2);
    assert_eq!(ci_approval_of(&events, "docs"), None);
    // An approval does not change which stages wait for CI.
    assert_eq!(
        stages_waiting_for_ci(&events),
        vec!["implement".to_string()]
    );
}

#[test]
fn an_approval_with_an_unknown_mode_or_a_branch_outside_natamux_ci_is_none() {
    let mut bad_mode = approved(1, "implement", "aaa");
    bad_mode.event.payload.insert("mode".into(), json!("all"));
    assert_eq!(ci_approval_of(&[bad_mode], "implement"), None);
    let mut bad_branch = approved(1, "implement", "aaa");
    bad_branch
        .event
        .payload
        .insert("branch".into(), json!("main"));
    assert_eq!(ci_approval_of(&[bad_branch], "implement"), None);
}
