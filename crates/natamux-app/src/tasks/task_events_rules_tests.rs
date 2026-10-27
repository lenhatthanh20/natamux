use natamux_proto::events::task_event::EVENT_TYPES;
use serde_json::json;

use super::*;

fn payload(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn describe(kind: &str, value: Value) -> String {
    describe_event(kind, &payload(value), &PathNames::new())
}

fn stored(seq: u64, kind: &str, source: &str, accepted: bool, value: Value) -> StoredEvent {
    StoredEvent {
        seq,
        accepted,
        event: TaskEvent {
            task_id: "T-0001".into(),
            source: source.into(),
            kind: kind.into(),
            at: format!("2026-10-08T09:00:{seq:02}.000Z"),
            payload: payload(value),
        },
    }
}

fn human(seq: u64, kind: &str) -> StoredEvent {
    stored(seq, kind, "human", true, json!({}))
}

fn texts(lines: &[ActivityLine]) -> Vec<String> {
    lines.iter().map(describe_line).collect()
}

fn group(events: &[StoredEvent]) -> Vec<ActivityLine> {
    group_events(events, &PathNames::new())
}

#[test]
fn has_a_sentence_for_every_kind_of_event() {
    for kind in EVENT_TYPES {
        assert!(
            event_sentence(kind).is_some_and(|text| !text.is_empty()),
            "{kind}"
        );
    }
    assert_eq!(EVENT_SENTENCE.len(), EVENT_TYPES.len());
}

#[test]
fn names_a_change_of_the_skip_mark_and_a_planned_skip_as_any_skip() {
    assert_eq!(describe("stage.skip-set", json!({})), "Skip choice changed");
    assert_eq!(describe("stage.skipped", json!({})), "Stage skipped");
}

#[test]
fn does_not_give_two_kinds_of_event_the_same_sentence() {
    let mut sentences: Vec<&str> = EVENT_TYPES
        .iter()
        .filter_map(|kind| event_sentence(kind))
        .collect();
    sentences.sort_unstable();
    sentences.dedup();
    assert_eq!(sentences.len(), EVENT_TYPES.len());
}

#[test]
fn says_a_send_back_by_ci_came_from_ci_also_when_it_goes_to_the_same_stage() {
    let from_ci = json!({"to": "implement", "from": "implement", "byAgent": true, "ci": true});
    assert_eq!(describe("stage.reopened", from_ci), "Sent back by CI");
    assert_eq!(
        describe("stage.reopened", json!({"to": "plan", "reason": "x"})),
        "Sent back to an earlier stage"
    );
    assert_eq!(
        describe("stage.started", json!({"ci": true})),
        "Stage started"
    );
}

#[test]
fn says_a_batch_of_review_comments_apart_from_a_one_off_comment() {
    assert_eq!(
        describe("review.commented", json!({"comments": ["c1", "c2"]})),
        "Review comments sent to the agent"
    );
    assert_eq!(
        describe("review.commented", json!({"text": "Fix it"})),
        "Review comment sent to the agent"
    );
}

#[test]
fn says_how_long_a_paused_agent_was_idle() {
    assert_eq!(
        describe("agent.paused", json!({"terminal": "1", "idleMinutes": 30})),
        "Agent paused after 30 min idle"
    );
    assert_eq!(
        describe("agent.paused", json!({})),
        "Agent paused while idle"
    );
}

#[test]
fn ends_with_an_arrow_and_the_name_of_the_path_an_event_chose() {
    let names = PathNames::from([("small".to_string(), "Small change".to_string())]);
    let path = payload(json!({"stage": "plan", "path": "small"}));
    assert_eq!(
        describe_event("stage.approved", &path, &names),
        "Stage approved → Small change"
    );
}

#[test]
fn names_the_path_by_its_id_when_no_name_is_known_and_adds_nothing_without_a_path() {
    assert_eq!(
        describe("stage.approved", json!({"path": "small"})),
        "Stage approved → small"
    );
    let names = PathNames::from([("small".to_string(), "Small change".to_string())]);
    let plain = payload(json!({"stage": "plan"}));
    assert_eq!(
        describe_event("stage.approved", &plain, &names),
        "Stage approved"
    );
}

#[test]
fn shows_the_path_in_grouped_lines() {
    let event = stored(1, "stage.approved", "human", true, json!({"path": "small"}));
    let names = PathNames::from([("small".to_string(), "Small change".to_string())]);
    let lines = group_events(&[event], &names);
    assert_eq!(
        describe_line(&lines[0]),
        "Stage approved → Small change · You"
    );
}

#[test]
fn shows_hours_minutes_and_seconds() {
    assert_eq!(format_clock("2026-10-08T09:05:07.000Z", 0), "09:05:07 AM");
    assert_eq!(format_clock("2026-10-08T21:05:07.000Z", 0), "09:05:07 PM");
    assert_eq!(format_clock("2026-10-08T23:05:07.000Z", 60), "12:05:07 AM");
    assert_eq!(format_clock("nope", 0), "");
}

#[test]
fn names_the_user_the_agent_and_the_daemon() {
    assert_eq!(actor_of("human"), "You");
    assert_eq!(actor_of("agent"), "Agent");
    assert_eq!(actor_of("system"), "Daemon");
    assert_eq!(actor_of("git"), "Daemon");
    assert_eq!(actor_of("ci"), "Daemon");
}

#[test]
fn joins_events_of_the_same_kind_in_a_row_and_counts_them() {
    let lines = group(&[
        stored(1, "task.created", "system", true, json!({})),
        human(2, "task.edited"),
        human(3, "task.edited"),
        human(4, "task.edited"),
        human(5, "task.edited"),
    ]);
    assert_eq!(
        texts(&lines),
        ["Task created · Daemon", "Details edited ×4 · You"]
    );
    assert_eq!(lines[1].at, "2026-10-08T09:00:05.000Z");
}

#[test]
fn keeps_events_apart_when_another_event_comes_between_them() {
    let lines = group(&[
        human(1, "task.edited"),
        stored(2, "git.committed", "git", true, json!({})),
        human(3, "task.edited"),
    ]);
    assert_eq!(lines.len(), 3);
}

#[test]
fn keeps_events_apart_when_the_actor_or_the_answer_of_the_daemon_differs() {
    let by_agent = stored(2, "task.edited", "agent", true, json!({}));
    assert_eq!(group(&[human(1, "task.edited"), by_agent]).len(), 2);
    let ignored = stored(2, "agent.resumed", "human", false, json!({}));
    assert_eq!(group(&[human(1, "agent.resumed"), ignored]).len(), 2);
}

#[test]
fn keeps_a_send_back_by_ci_apart_from_one_by_a_person() {
    let reopened = |seq, value| stored(seq, "stage.reopened", "system", true, value);
    let lines = group(&[
        reopened(1, json!({"to": "implement", "ci": true})),
        reopened(2, json!({"to": "implement", "ci": true})),
        reopened(3, json!({"to": "implement"})),
    ]);
    assert_eq!(
        texts(&lines),
        [
            "Sent back by CI ×2 · Daemon",
            "Sent back to an earlier stage · Daemon"
        ]
    );
}

#[test]
fn says_why_a_workspace_could_not_be_made() {
    let failed = stored(
        1,
        "workspace.failed",
        "system",
        true,
        json!({"message": "index.lock exists"}),
    );
    assert_eq!(
        group(&[failed])[0].detail.as_deref(),
        Some("index.lock exists")
    );
}

#[test]
fn has_no_lines_without_events() {
    assert!(group(&[]).is_empty());
}

#[test]
fn lists_all_the_background_work_the_agent_waits_for_from_the_newest_event_of_a_run() {
    let waiting = |seq, list| {
        stored(
            seq,
            "agent.waiting",
            "agent",
            true,
            json!({"waitingFor": list}),
        )
    };
    let lines = group(&[
        waiting(1, json!(["Bash: pnpm lint"])),
        waiting(
            2,
            json!([
                "Bash: pnpm test",
                "subagent: Review diff",
                "scheduled prompt: check CI"
            ]),
        ),
    ]);
    assert_eq!(
        texts(&lines),
        [
            "Agent waits for its background work ×2: Bash: pnpm test; subagent: Review diff; \
          scheduled prompt: check CI · Agent"
        ]
    );
    assert_eq!(
        lines[0].detail.as_deref(),
        Some("Bash: pnpm test; subagent: Review diff; scheduled prompt: check CI")
    );
}

#[test]
fn has_no_detail_for_other_events_or_for_a_list_it_cannot_read() {
    let odd = stored(
        1,
        "agent.waiting",
        "agent",
        true,
        json!({"waitingFor": "Bash: ls"}),
    );
    let lines = group(&[odd, stored(2, "agent.finished", "agent", true, json!({}))]);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.detail.clone())
            .collect::<Vec<_>>(),
        [None, None]
    );
}

#[test]
fn lists_1000_events_newest_first_in_order() {
    let events: Vec<StoredEvent> = (0..1000)
        .map(|index| {
            let (kind, source) = if index % 2 == 0 {
                ("task.edited", "human")
            } else {
                ("agent.finished", "agent")
            };
            stored(index + 1, kind, source, true, json!({}))
        })
        .collect();
    let shown = texts(&timeline_lines(&events));
    assert_eq!(shown.len(), 1000);
    assert_eq!(shown[0], "Agent finished its turn · Agent");
    assert_eq!(shown[1], "Details edited · You");
    assert_eq!(shown[999], "Details edited · You");
}

#[test]
fn leaves_out_the_agent_is_working_lines_and_says_so_when_there_is_no_activity() {
    let events = [
        human(1, "task.edited"),
        stored(2, "agent.active", "agent", true, json!({})),
    ];
    assert_eq!(texts(&timeline_lines(&events)), ["Details edited · You"]);
    assert!(timeline_lines(&[]).is_empty());
    assert_eq!(NO_ACTIVITY, "No activity yet.");
}
