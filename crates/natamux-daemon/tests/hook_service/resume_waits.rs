use super::*;

struct Agent {
    id: &'static str,
    prompt: &'static str,
    before: &'static str,
    after: &'static str,
    stop: &'static str,
    permission: Option<Value>,
}

fn agents() -> Vec<Agent> {
    vec![
        Agent {
            id: "claude-code",
            prompt: "UserPromptSubmit",
            before: "PreToolUse",
            after: "PostToolUse",
            stop: "Stop",
            permission: Some(hook("PermissionRequest", json!({}))),
        },
        Agent {
            id: "codex",
            prompt: "UserPromptSubmit",
            before: "PreToolUse",
            after: "PostToolUse",
            stop: "Stop",
            permission: Some(hook("PermissionRequest", json!({}))),
        },
        Agent {
            id: "gemini",
            prompt: "BeforeAgent",
            before: "BeforeTool",
            after: "AfterTool",
            stop: "AfterAgent",
            permission: Some(hook(
                "Notification",
                json!({ "notification_type": "ToolPermission" }),
            )),
        },
        Agent {
            id: "copilot",
            prompt: "userPromptSubmitted",
            before: "preToolUse",
            after: "postToolUse",
            stop: "agentStop",
            permission: Some(
                json!({ "natamux_event": "notification", "notification_type": "permission_prompt" }),
            ),
        },
        Agent {
            id: "cursor",
            prompt: "beforeSubmitPrompt",
            before: "beforeShellExecution",
            after: "afterShellExecution",
            stop: "stop",
            permission: None,
        },
    ]
}

impl Agent {
    fn launch(&self, fx: &Fx, id: &str, session: Option<&str>) {
        fx.tasks
            .record_agent_launch(id, self.id, None, None, None, ("1", session))
            .unwrap();
    }

    fn raw(&self, event: &str) -> Value {
        if self.id == "copilot" {
            json!({ "natamux_event": event, "sessionId": "same-session" })
        } else {
            hook(
                event,
                json!({ "session_id": "same-session", "conversation_id": "same-session" }),
            )
        }
    }

    fn send(&self, service: &HookService, id: &str, event: &str) -> HookResult {
        service
            .handle(self.id, &self.raw(event), Some(id), None)
            .unwrap()
    }

    fn ask(&self, service: &HookService, fx: &Fx, id: &str) {
        if let Some(raw) = &self.permission {
            let mut raw = raw.clone();
            for key in ["sessionId", "session_id", "conversation_id"] {
                raw[key] = json!("same-session");
            }
            service.handle(self.id, &raw, Some(id), None).unwrap();
        } else {
            fx.apply(
                id,
                json!({
                    "source": "agent", "type": "agent.asked",
                    "payload": { "reason": "terminal bell", "terminal": "1" },
                }),
            );
        }
        assert_eq!(fx.state(id), TaskState::NeedsInput, "{}", self.id);
    }
}

#[test]
fn every_agent_resumes_after_a_submitted_answer() {
    for agent in agents() {
        let fx = Fx::new();
        let service = fx.service();
        let id = fx.make("Answer").id;
        agent.send(&service, &id, agent.prompt);
        fx.apply(
            &id,
            json!({ "source": "agent", "type": "agent.asked",
            "payload": { "question": "Which branch?" } }),
        );
        assert_eq!(
            agent.send(&service, &id, agent.prompt).applied,
            ["agent.resumed"],
            "{}",
            agent.id
        );
        assert_eq!(fx.state(&id), TaskState::Running);
    }
}

#[test]
fn every_agent_recovers_a_permission_wait_after_restart_only_after_the_tool_runs() {
    for agent in agents() {
        let fx = Fx::new();
        let service = fx.service();
        let id = fx.make("Permission").id;
        agent.send(&service, &id, agent.prompt);
        agent.ask(&service, &fx, &id);
        drop(service);
        agent.launch(&fx, &id, Some("same-session"));
        let service = fx.service();
        assert!(
            agent.send(&service, &id, agent.before).applied.is_empty(),
            "{}",
            agent.id
        );
        assert_eq!(fx.state(&id), TaskState::NeedsInput);
        assert_eq!(
            agent.send(&service, &id, agent.after).applied,
            ["agent.resumed"],
            "{}",
            agent.id
        );
        assert_eq!(fx.state(&id), TaskState::Running);
        assert!(agent.send(&service, &id, agent.after).applied.is_empty());
        assert_eq!(
            fx.kinds(&id)
                .iter()
                .filter(|kind| *kind == "agent.resumed")
                .count(),
            1
        );
        assert_eq!(fx.last_payload(&id)["stage"], "implement");
    }
}

#[test]
fn every_agent_ends_a_denied_permission_wait_on_turn_end() {
    for agent in agents() {
        let fx = Fx::new();
        let service = fx.service();
        let id = fx.make("Denied").id;
        agent.send(&service, &id, agent.prompt);
        agent.ask(&service, &fx, &id);
        agent.launch(&fx, &id, Some("same-session"));
        let restarted = fx.service();
        assert_eq!(
            agent.send(&restarted, &id, agent.stop).applied,
            ["agent.resumed", "agent.finished"],
            "{}",
            agent.id
        );
        assert_eq!(fx.state(&id), TaskState::Idle);
    }
}

#[test]
fn an_explicit_question_after_a_permission_stays_waiting_even_after_restart() {
    for agent in agents() {
        let fx = Fx::new();
        let service = fx.service();
        let id = fx.make("Question").id;
        agent.send(&service, &id, agent.prompt);
        agent.ask(&service, &fx, &id);
        // The state is already needs-input, so this event is logged but does not move it.
        fx.apply(
            &id,
            json!({ "source": "agent", "type": "agent.asked",
            "payload": { "question": "Which branch?", "terminal": "1" } }),
        );
        agent.launch(&fx, &id, Some("same-session"));
        for service in [&service, &fx.service()] {
            for event in [agent.before, agent.after, agent.stop] {
                assert!(
                    agent.send(service, &id, event).applied.is_empty(),
                    "{}: {event}",
                    agent.id
                );
            }
            assert_eq!(fx.state(&id), TaskState::NeedsInput);
        }
        assert_eq!(
            agent.send(&service, &id, agent.prompt).applied,
            ["agent.resumed"]
        );
    }
}

#[test]
fn a_new_launch_does_not_inherit_a_permission_wait_from_an_old_run() {
    for agent in agents() {
        for session in [None, Some("different-session")] {
            let fx = Fx::new();
            let service = fx.service();
            let id = fx.make("New session").id;
            agent.send(&service, &id, agent.prompt);
            agent.ask(&service, &fx, &id);
            agent.launch(&fx, &id, session);
            let restarted = fx.service();
            for event in [agent.after, agent.stop] {
                assert!(
                    agent.send(&restarted, &id, event).applied.is_empty(),
                    "{}",
                    agent.id
                );
            }
            assert_eq!(fx.state(&id), TaskState::NeedsInput);
        }
    }
}

#[test]
fn an_unknown_wait_does_not_reuse_a_permission_from_an_earlier_run() {
    let fx = Fx::new();
    let service = fx.service();
    let id = fx.make("New run").id;
    step(&service, &id, "UserPromptSubmit");
    step(&service, &id, "PermissionRequest");
    fx.apply(
        &id,
        json!({ "source": "human", "type": "state.overridden",
        "payload": { "to": "needs-input" } }),
    );
    assert!(step(&service, &id, "PostToolUse").applied.is_empty());
    assert_eq!(fx.state(&id), TaskState::NeedsInput);
}
