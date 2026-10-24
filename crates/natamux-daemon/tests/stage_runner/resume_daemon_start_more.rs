//! The second half of `describe('when the daemon starts')`.

use super::*;

#[test]
fn records_the_reason_for_each_agent_that_cannot_start() {
    let f = Fixture::new();
    let review = in_review_after_restart(&f, "s-p", "claude-code");
    let running = running_after_restart(&f, "s-q", "claude-code");
    f.fail_spawn(|_, _| true);

    resume_all(&f);

    for task in [&review, &running] {
        check_failed_with_no_shell(&f, &task.id, "spec");
    }
    assert_eq!(f.task(&review.id).state, TaskState::Review);
    assert_eq!(f.task(&running.id).state, TaskState::NeedsInput);
}

#[test]
fn keeps_the_other_lane_of_a_group_going_when_a_lane_in_review_cannot_start() {
    let f = Fixture::new();
    let built = build_after_restart(&f);
    let task = to_review(&f, &built.id, "implement");
    f.fail_spawn(|_, terminal| terminal == "1");

    resume_all(&f);

    assert_eq!(shells(&f, &task.id), ["2"]);
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
    let lanes = json!({ "implement": "review", "test-spec": "needs-input" });
    assert_eq!(lanes_of(&f, &task.id), lanes);
}

#[test]
fn goes_on_with_the_newest_session_of_the_folder_before_the_stage_reported_one() {
    let f = Fixture::new();
    let id = f
        .created(json!({ "title": "Export tasks", "agent": "claude-code" }))
        .id;
    let task = f.with_workspace(&id);
    assert_eq!(task.state, TaskState::Running);
    f.terminals.close(&task.id);

    resume_all(&f);

    assert!(script_has(&f, &task.id, "1", "--continue"));
    assert!(!script_has(&f, &task.id, "1", "Task: Export tasks"));
}

#[test]
fn goes_on_with_the_session_of_each_stage_of_a_group_in_its_own_terminal() {
    let f = Fixture::new();
    let task = build_after_restart(&f);

    resume_all(&f);

    assert_eq!(shells(&f, &task.id), ["1", "2"]);
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
    let lanes = json!({ "implement": "needs-input", "test-spec": "needs-input" });
    assert_eq!(lanes_of(&f, &task.id), lanes);
}

// Both stages run in the same folder, so its newest session may be the other stage's.
#[test]
fn starts_a_stage_of_a_group_with_no_session_with_its_prompt() {
    let f = Fixture::new();
    let id = build_ready(&f);
    f.terminals.close(&id);

    resume_all(&f);

    for terminal in ["1", "2"] {
        let script = f.launch_script(&id, terminal);
        assert!(script.contains("Task: Export tasks"), "{terminal}");
        assert!(starts_fresh(&script), "{terminal}");
    }
}

#[test]
fn starts_each_other_agent_again_with_the_stage_prompt() {
    for agent in OTHER_AGENTS {
        let f = Fixture::new();
        let task = running_after_restart(&f, "s-1", agent);

        resume_all(&f);

        assert!(typed_launch(&f, &task.id), "{agent}");
        let script = f.launch_script(&task.id, "1");
        assert!(script.contains("Task: Export tasks"), "{agent}");
        assert!(starts_fresh(&script), "{agent}");
    }
}

#[test]
fn leaves_alone_the_tasks_no_agent_works_on() {
    for to in ["running", "review"] {
        let f = Fixture::new();
        let no_workspace = f.created(json!({ "title": "Later", "agent": "claude-code" }));
        let payload = json!({ "to": to, "stage": "spec" });
        ev(
            &f,
            &no_workspace.id,
            "human",
            "state.overridden",
            payload.clone(),
        );
        // A person runs their own agent in the shell; Natamux never started one.
        let by_hand = f.created(json!({ "title": "By hand" }));
        f.runner.run.leave_to_person(&by_hand.id).unwrap();
        f.with_workspace(&by_hand.id);
        ev(&f, &by_hand.id, "human", "state.overridden", payload);
        f.terminals.close(&by_hand.id);
        let ids = [no_workspace.id.clone(), by_hand.id.clone()];
        let before: Vec<usize> = ids.iter().map(|id| event_count(&f, id)).collect();

        resume_all(&f);

        for (at, id) in ids.iter().enumerate() {
            assert!(shells(&f, id).is_empty(), "{to}");
            assert_eq!(event_count(&f, id), before[at], "{to}");
            assert_eq!(f.task(id).state, state_of(to), "{to}");
        }
    }
}

#[test]
fn starts_the_other_agents_when_one_cannot_start() {
    let f = Fixture::new();
    let first = running_after_restart(&f, "s-1", "claude-code");
    let second = running_after_restart(&f, "s-2", "claude-code");
    let failed = AtomicBool::new(false);
    f.fail_spawn(move |_, _| !failed.swap(true, Ordering::SeqCst));

    resume_all(&f);

    // The failed one waits for you with the reason, whichever came first.
    let after: Vec<(TaskState, Vec<String>)> = [&first, &second]
        .iter()
        .map(|task| (f.task(&task.id).state, shells(&f, &task.id)))
        .collect();
    assert!(after.contains(&(TaskState::NeedsInput, vec![])));
    assert!(after.contains(&(TaskState::Running, vec!["1".to_string()])));
}

#[test]
fn adds_only_the_launch_to_the_events_and_keeps_each_state() {
    let f = Fixture::new();
    let made = [
        running_after_restart(&f, "s-1", "claude-code"),
        waiting_after_restart(&f, json!({ "sessionId": "s-2" }), "claude-code"),
        in_review_after_restart(&f, "s-3", "claude-code"),
    ];
    let before: Vec<usize> = made.iter().map(|task| event_count(&f, &task.id)).collect();

    resume_all(&f);

    for (at, task) in made.iter().enumerate() {
        let types = event_types(&f, &task.id);
        assert_eq!(types[before[at]..], ["agent.launched"]);
        assert_eq!(f.task(&task.id).state, task.state);
    }
}

#[test]
fn resumes_a_running_and_a_waiting_stage_of_the_same_group() {
    let f = Fixture::new();
    let task = build_after_restart(&f);
    let payload = json!({ "stage": "implement", "sessionId": "s-1" });
    ev(&f, &task.id, "agent", "agent.resumed", payload);

    resume_all(&f);

    let lanes = json!({ "implement": "running", "test-spec": "needs-input" });
    assert_eq!(lanes_of(&f, &task.id), lanes);
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
    assert!(script_has(&f, &task.id, "2", "--resume 's-2'"));
}

#[test]
fn leaves_alone_a_task_that_is_done_cancelled_failed_blocked_or_a_draft() {
    for to in ["done", "cancelled", "failed", "blocked", "draft"] {
        let f = Fixture::new();
        let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
        ev(
            &f,
            &task.id,
            "human",
            "state.overridden",
            json!({ "to": to, "stage": "spec" }),
        );
        f.terminals.close(&task.id);
        let before = event_count(&f, &task.id);

        resume_all(&f);

        assert!(shells(&f, &task.id).is_empty(), "{to}");
        assert_eq!(event_count(&f, &task.id), before, "{to}");
    }
}

// Natamux cannot start it, so it must not fail the stage either.
#[test]
fn leaves_alone_a_task_whose_agent_natamux_cannot_start() {
    let f = Fixture::new();
    let task = running_after_restart(&f, "s-1", "opencode");
    let before = event_count(&f, &task.id);

    resume_all(&f);

    assert_eq!(f.task(&task.id).state, TaskState::Running);
    assert!(shells(&f, &task.id).is_empty());
    assert_eq!(event_count(&f, &task.id), before);
}

#[test]
fn pastes_the_prompt_of_a_new_start_of_the_stage_into_the_resumed_agent() {
    let f = Fixture::new();
    let task = waiting_after_restart(&f, json!({ "sessionId": "s-1" }), "claude-code");
    resume_all(&f);
    let pty = last_pty(&f);

    let input = StageActionInput {
        to: Some("spec".into()),
        ..StageActionInput::default()
    };
    f.runner
        .run
        .act(&task.id, StageAction::Reopen, &input)
        .unwrap();
    f.settle();

    expect_submitted(&pty, "Task: Export tasks");
    let typed = pty.written().concat();
    assert_eq!(
        typed.split(&format!("agent-launch-{}.sh", task.id)).count(),
        2
    );
}

#[test]
fn resumes_again_after_a_second_restart() {
    let f = Fixture::new();
    let task = running_after_restart(&f, "s-1", "claude-code");
    resume_all(&f);
    f.terminals.close(&task.id);

    resume_all(&f);

    assert_eq!(shells(&f, &task.id), ["1"]);
    assert!(script_has(&f, &task.id, "1", "--resume 's-1'"));
}
