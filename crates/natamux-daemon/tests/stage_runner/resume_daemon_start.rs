//! `describe('when the daemon starts')` of `stage-runner.test.ts`.

use std::sync::atomic::{AtomicBool, Ordering};

use natamux_daemon::stages::act::StageActionInput;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use serde_json::json;

// The temp folder name has "s-1" in it ("stages-1"), so the checks use the quoted session.
use crate::resume_restart::*;
use crate::{Fixture, expect_submitted};

fn resume_all(f: &Fixture) {
    f.runner.run.resume_agents();
    f.settle();
}

/// The agent was at work on the spec, after a turn that reported its session.
fn running_after_restart(f: &Fixture, session: &str, agent: &str) -> Task {
    let task = waiting_after_restart(f, json!({ "sessionId": session }), agent);
    let payload = json!({ "stage": "spec", "sessionId": session });
    ev(f, &task.id, "agent", "agent.resumed", payload);
    f.task(&task.id)
}

/// Puts a stage in review, as when it waits for approval, with no shell left.
fn to_review(f: &Fixture, id: &str, stage: &str) -> Task {
    ev(
        f,
        id,
        "human",
        "state.overridden",
        json!({ "to": "review", "stage": stage }),
    );
    f.terminals.close(id);
    f.task(id)
}

/// The agent said the stage is done; the stage waits for approval.
fn in_review_after_restart(f: &Fixture, session: &str, agent: &str) -> Task {
    let waiting = waiting_after_restart(f, json!({ "sessionId": session }), agent);
    to_review(f, &waiting.id, "spec")
}

fn build_ready(f: &Fixture) -> String {
    let id = f
        .created(json!({ "title": "Export tasks", "agent": "claude-code" }))
        .id;
    ev(
        f,
        &id,
        "human",
        "state.overridden",
        json!({ "to": "ready", "stage": "build" }),
    );
    f.with_workspace(&id);
    id
}
#[test]
fn goes_on_with_the_session_of_a_running_stage_with_no_new_prompt() {
    let f = Fixture::new();
    let task = running_after_restart(&f, "s-1", "claude-code");
    assert_eq!(task.state, TaskState::Running);

    resume_all(&f);

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(typed_launch(&f, &task.id));
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(!script_has(&f, &task.id, "1", "Task: Export tasks"));
}

#[test]
fn goes_on_with_the_session_of_a_stage_that_waits_for_you_and_keeps_it_waiting() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");

    resume_all(&f);

    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
}

#[test]
fn goes_on_with_the_session_of_a_stage_in_review_and_keeps_it_in_review() {
    let f = Fixture::new();
    let task = in_review_after_restart(&f, "s-1", "claude-code");
    assert_eq!(task.state, TaskState::Review);

    resume_all(&f);

    assert_eq!(f.task(&task.id).state, TaskState::Review);
    assert!(typed_launch(&f, &task.id));
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(!script_has(&f, &task.id, "1", "Task: Export tasks"));
}

#[test]
fn goes_on_with_the_last_session_of_a_stage_in_review() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
    let payload = json!({ "stage": "spec", "sessionId": "s-2" });
    ev(&f, &task.id, "agent", "agent.resumed", payload.clone());
    ev(&f, &task.id, "agent", "agent.finished", payload);
    to_review(&f, &task.id, "spec");

    resume_all(&f);

    assert!(script_has(&f, &task.id, "1", "--resume 's-2'"));
    assert!(!script_has(&f, &task.id, "1", "'s-1'"));
}

#[test]
fn goes_on_with_the_newest_session_of_the_folder_for_a_review_with_none() {
    let f = Fixture::new();
    let waiting = waiting_after_restart(&f, json!({}), "claude-code");
    let task = to_review(&f, &waiting.id, "spec");

    resume_all(&f);

    assert!(script_has(&f, &task.id, "1", "--continue"));
    assert!(!script_has(&f, &task.id, "1", "Task: Export tasks"));
    assert_eq!(f.task(&task.id).state, TaskState::Review);
}

#[test]
fn leaves_the_agent_of_a_stage_in_review_alone_on_a_second_start() {
    let f = Fixture::new();
    let task = in_review_after_restart(&f, "s-1", "claude-code");
    resume_all(&f);
    for pty in f.ptys() {
        pty.set_program(Some("node"));
    }
    let before = event_count(&f, &task.id);
    let written: Vec<usize> = f.ptys().iter().map(|pty| pty.written().len()).collect();

    resume_all(&f);

    assert_eq!(event_count(&f, &task.id), before);
    let after: Vec<usize> = f.ptys().iter().map(|pty| pty.written().len()).collect();
    assert_eq!(after, written);
    assert_eq!(f.task(&task.id).state, TaskState::Review);
}

#[test]
fn starts_codex_in_review_or_running_again_with_the_stage_prompt_even_with_a_session() {
    for in_review in [true, false] {
        let f = Fixture::new();
        let task = if in_review {
            in_review_after_restart(&f, "s-1", "codex")
        } else {
            running_after_restart(&f, "s-1", "codex")
        };

        resume_all(&f);

        assert_eq!(f.task(&task.id).state, task.state);
        let script = f.launch_script(&task.id, "1");
        assert!(script.starts_with("codex "));
        assert!(script.contains("Task: Export tasks"));
        assert!(starts_fresh(&script));
    }
}

#[test]
fn goes_on_with_the_session_of_a_lane_in_review_and_of_a_waiting_lane() {
    let f = Fixture::new();
    let built = build_after_restart(&f);
    let task = to_review(&f, &built.id, "implement");
    let lanes = json!({ "implement": "review", "test-spec": "needs-input" });
    assert_eq!(serde_json::Value::Object(task.lanes.clone()), lanes);

    resume_all(&f);

    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(!script_has(&f, &task.id, "1", "'s-2'"));
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
    assert!(!script_has(&f, &task.id, "2", "'s-1'"));
    assert_eq!(f.task(&task.id).state, task.state);
    assert_eq!(lanes_of(&f, &task.id), lanes);
}

// Both lanes work in the same folder, so its newest session may be the other lane's.
#[test]
fn starts_a_lane_of_a_group_in_review_with_no_session_with_its_prompt() {
    let f = Fixture::new();
    let id = build_ready(&f);
    let task = to_review(&f, &id, "implement");

    resume_all(&f);

    assert!(script_has(&f, &task.id, "1", "Task: Export tasks"));
    assert!(starts_fresh(&f.launch_script(&task.id, "1")));
    assert_eq!(lanes_of(&f, &task.id)["implement"], "review");
}

#[test]
fn leaves_alone_a_blocked_or_failed_lane_beside_one_in_review() {
    for to in ["blocked", "failed"] {
        let f = Fixture::new();
        let built = build_after_restart(&f);
        let task = to_review(&f, &built.id, "implement");
        ev(
            &f,
            &task.id,
            "human",
            "state.overridden",
            json!({ "to": to, "stage": "test-spec" }),
        );
        f.terminals.close(&task.id);

        resume_all(&f);

        assert_eq!(shells(&f, &task.id), ["1"], "{to}");
        assert!(script_has(&f, &task.id, "1", "--resume 's-1'"), "{to}");
    }
}

#[test]
fn goes_on_with_the_session_of_each_stage_of_a_group_in_review() {
    let f = Fixture::new();
    let task = build_after_restart(&f);
    ev(
        &f,
        &task.id,
        "human",
        "state.overridden",
        json!({ "to": "review" }),
    );
    let both = json!({ "implement": "review", "test-spec": "review" });
    assert_eq!(lanes_of(&f, &task.id), both);

    resume_all(&f);

    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
    assert_eq!(lanes_of(&f, &task.id), both);
}

#[test]
fn leaves_alone_a_stage_in_review_that_no_agent_runs() {
    type Configure = fn(&Fixture);
    let rows: [(&str, &str, Configure); 3] = [
        ("a review by a person", "review", with_manual_review),
        ("the release", "release", |_| ()),
        ("tests a command runs", "testing", with_test_command),
    ];
    for (name, stage, configure) in rows {
        let f = Fixture::new();
        configure(&f);
        let other = in_review_after_restart(&f, "s-1", "claude-code");
        let task = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
        ev(
            &f,
            &task.id,
            "human",
            "state.overridden",
            json!({ "to": "review", "stage": stage }),
        );
        f.with_workspace(&task.id);
        f.terminals.close(&task.id);
        let before = event_count(&f, &task.id);

        resume_all(&f);

        assert!(shells(&f, &task.id).is_empty(), "{name}");
        assert_eq!(event_count(&f, &task.id), before, "{name}");
        let now = f.task(&task.id);
        assert_eq!(
            (now.stage.as_str(), now.state),
            (stage, TaskState::Review),
            "{name}"
        );
        assert!(script_has(&f, &other.id, "1", "--resume 's-1'"), "{name}");
    }
}

// Only a running stage takes the failed check. A stage in review already waits for you, so it
// stays there with no agent, as a waiting one does.
#[test]
fn keeps_a_stage_in_review_when_its_agent_cannot_start_first_or_last() {
    for review_first in [true, false] {
        let f = Fixture::new();
        let early = (!review_first).then(|| running_after_restart(&f, "s-q", "claude-code"));
        let review = in_review_after_restart(&f, "s-p", "claude-code");
        let running = match early {
            Some(task) => task,
            None => running_after_restart(&f, "s-q", "claude-code"),
        };
        let review_id = review.id.clone();
        f.fail_spawn(move |task_id, _| task_id == review_id);

        resume_all(&f);

        assert_eq!(
            f.task(&review.id).state,
            TaskState::Review,
            "{review_first}"
        );
        assert!(shells(&f, &review.id).is_empty(), "{review_first}");
        let (accepted, _, _) = last_event(&f, &review.id);
        assert!(!accepted, "{review_first}");
        check_failed_with_no_shell(&f, &review.id, "spec");
        assert_eq!(
            f.task(&running.id).state,
            TaskState::Running,
            "{review_first}"
        );
        assert!(
            script_has(&f, &running.id, "1", "--resume 's-q'"),
            "{review_first}"
        );
    }
}

#[path = "resume_daemon_start_more.rs"]
mod more;
