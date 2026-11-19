#![allow(clippy::unwrap_used)]

use super::*;
use natamux_proto::events::task_event::TaskEvent;
use serde_json::json;

fn event(kind: &str, accepted: bool, payload: Value) -> StoredEvent {
    StoredEvent {
        seq: 1,
        accepted,
        event: TaskEvent {
            task_id: "T-1".into(),
            source: "agent".into(),
            kind: kind.into(),
            at: "2026-11-09T00:00:00.000Z".into(),
            payload: payload.as_object().unwrap().clone(),
        },
    }
}

fn bell() -> StoredEvent {
    event(
        "agent.asked",
        true,
        json!({ "stage": "implement", "reason": "terminal bell", "terminal": "1" }),
    )
}

fn wait(events: &[StoredEvent]) -> Wait {
    wait_in(events, "implement", "build", "1").unwrap_or_else(|whole| whole)
}

/// An event of another lane: it tells nothing about the wait of this one.
fn sibling() -> StoredEvent {
    event(
        "agent.resumed",
        true,
        json!({ "stage": "test-spec", "terminal": "2" }),
    )
}

/// `wait_at_end` over `events`, and the limits it asked for.
fn wait_at_end_of(events: &[StoredEvent]) -> (Wait, Vec<usize>) {
    let mut limits = Vec::new();
    let tail = |limit: usize| {
        limits.push(limit);
        Ok::<_, ()>(events[events.len().saturating_sub(limit)..].to_vec())
    };
    let found = wait_at_end(tail, "implement", "build", "1").unwrap();
    (found, limits)
}

#[test]
fn a_wait_near_the_end_of_a_long_log_is_found_in_its_last_events() {
    let mut events = vec![sibling(); 3_000];
    events.push(bell());
    events.extend(vec![sibling(); 5]);

    let (found, limits) = wait_at_end_of(&events);

    assert_eq!(found, Wait::Hook);
    assert_eq!(found, wait(&events));
    assert_eq!(limits, [TAIL]);
}

#[test]
fn a_wait_older_than_the_last_events_is_read_further_back() {
    let old = [vec![bell()], vec![sibling(); 200]].concat();
    assert_eq!(wait_at_end_of(&old), (Wait::Hook, vec![TAIL, TAIL * 8]));

    // The session of the wait is older than the first read: the restore of another session.
    let asked = event(
        "agent.asked",
        true,
        json!({ "stage": "implement", "hook": "PermissionRequest", "sessionId": "old" }),
    );
    let launch = event(
        "agent.launched",
        true,
        json!({ "terminal": "1", "resumeSessionId": "new" }),
    );
    // At 138 the first read ends between the wait and the event that names its session.
    for (at, reads) in [(0, 2), (138, 2), (150, 1)] {
        let mut events = vec![sibling(); 200];
        events.insert(at, asked.clone());
        events.insert(at + 1, bell());
        events.push(launch.clone());
        let (found, limits) = wait_at_end_of(&events);
        assert_eq!(found, wait(&events), "{at}");
        assert_eq!(found, Wait::Answer, "{at}");
        assert_eq!(limits.len(), reads, "{at}");
    }
}

#[test]
fn a_log_with_no_wait_is_read_to_its_start_and_needs_an_answer() {
    for count in [0, 10, TAIL, 200] {
        let events = vec![sibling(); count];
        let (found, limits) = wait_at_end_of(&events);
        assert_eq!(found, Wait::Answer, "{count}");
        assert!(limits.len() <= 2, "{count}");
        assert!(limits.last().is_some_and(|last| *last > count), "{count}");
    }
}

#[test]
fn sibling_activity_and_overrides_do_not_change_the_owning_wait() {
    for kind in ["agent.asked", "agent.resumed", "state.overridden"] {
        let sibling = event(kind, true, json!({ "stage": "test-spec", "terminal": "2" }));
        assert_eq!(wait(&[bell(), sibling]), Wait::Hook, "{kind}");
    }
}

#[test]
fn a_bell_or_permission_from_another_stage_or_terminal_is_no_evidence() {
    for payload in [
        json!({ "stage": "test-spec", "reason": "terminal bell", "terminal": "1" }),
        json!({ "reason": "terminal bell", "terminal": "2" }),
        json!({ "stage": "spec", "hook": "PermissionRequest" }),
        json!({ "hook": "PermissionRequest" }),
    ] {
        assert_eq!(wait(&[event("agent.asked", true, payload)]), Wait::Answer);
    }
}

#[test]
fn an_unaccepted_bell_does_not_replace_an_explicit_question() {
    let question = event(
        "agent.asked",
        true,
        json!({ "stage": "implement", "message": "Which branch?" }),
    );
    let mut rejected = bell();
    rejected.accepted = false;
    assert_eq!(wait(&[question, rejected]), Wait::Answer);
}

#[test]
fn an_unaccepted_explicit_question_replaces_a_bell_wait() {
    let question = event(
        "agent.asked",
        false,
        json!({ "stage": "implement", "message": "Which branch?" }),
    );
    assert_eq!(wait(&[bell(), question]), Wait::Answer);
}

#[test]
fn new_runs_and_manual_waits_do_not_inherit_old_permissions() {
    for (kind, payload) in [
        ("stage.started", json!({ "stage": "implement" })),
        ("agent.resumed", json!({ "stage": "implement" })),
        ("agent.launched", json!({ "stage": "implement" })),
        (
            "stage.reopened",
            json!({ "stage": "review", "to": "build" }),
        ),
        (
            "state.overridden",
            json!({ "stage": "build", "to": "needs-input" }),
        ),
        ("state.overridden", json!({ "to": "needs-input" })),
        (
            "stage.check-failed",
            json!({ "stage": "implement", "final": true }),
        ),
    ] {
        assert_eq!(
            wait(&[bell(), event(kind, true, payload)]),
            Wait::Answer,
            "{kind}"
        );
    }
}

#[test]
fn named_session_restores_preserve_known_waits_but_not_explicit_or_unknown_waits() {
    for (asked, expected) in [
        (bell(), Wait::Hook),
        (
            event(
                "agent.asked",
                true,
                json!({ "stage": "implement", "hook": "PermissionRequest", "sessionId": "same" }),
            ),
            Wait::Hook,
        ),
        (
            event(
                "agent.asked",
                true,
                json!({ "stage": "implement", "message": "Which branch?" }),
            ),
            Wait::Answer,
        ),
        (
            event("state.overridden", true, json!({ "to": "needs-input" })),
            Wait::Answer,
        ),
    ] {
        let launch = event(
            "agent.launched",
            true,
            json!({ "terminal": "1", "resumeSessionId": "same" }),
        );
        assert_eq!(wait(&[asked, launch]), expected);
    }
}

#[test]
fn a_siblings_restored_launch_does_not_discard_this_lanes_wait() {
    let launch = event(
        "agent.launched",
        true,
        json!({ "terminal": "2", "resumeSessionId": "other" }),
    );
    assert_eq!(wait(&[bell(), launch]), Wait::Hook);
}
