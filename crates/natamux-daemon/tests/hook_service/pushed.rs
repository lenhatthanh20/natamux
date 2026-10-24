//! The group 'AgentHookService, pushed to the dashboards'. Node's `workspaceChanged`, `sessionSeen`
//! and `usageSeen` are `vi.fn`-style arrays there; here they are closures that record their
//! calls, and the tests check those records.

use std::sync::atomic::AtomicBool;

use serde_json::Map;

use super::*;

struct Push {
    fx: Fx,
    touched: Arc<Mutex<Vec<String>>>,
}

fn setup() -> Push {
    Push {
        fx: Fx::new(),
        touched: Arc::default(),
    }
}

fn edit(path: &str) -> Value {
    hook(
        "PreToolUse",
        json!({ "tool_name": "Edit", "tool_input": { "file_path": path } }),
    )
}

fn pairs(lines: &[(&str, &str)]) -> Vec<(String, String)> {
    lines
        .iter()
        .map(|(name, text)| (name.to_string(), text.to_string()))
        .collect()
}

impl Push {
    /// `hooks(stages)`: the bus, and `workspaceChanged` that records the task.
    fn hooks(&self, stages: Option<Arc<dyn StageLookup>>) -> HookService {
        let touched = Arc::clone(&self.touched);
        let mut options = self.fx.options(true);
        options.workspace_changed = Some(Arc::new(move |task: &str| {
            touched.lock().unwrap().push(task.to_string());
        }));
        let stages = stages.unwrap_or_else(|| Arc::new(NoStages));
        self.fx.service_with(stages, options)
    }

    fn touched(&self) -> Vec<String> {
        self.touched.lock().unwrap().clone()
    }

    fn activity_messages(&self) -> Vec<(String, Map<String, Value>)> {
        self.fx
            .messages()
            .into_iter()
            .filter_map(|message| match message {
                DaemonMessage::ActivityChanged { task_id, activity } => Some((task_id, activity)),
                _ => None,
            })
            .collect()
    }

    fn shells_changed(&self) -> usize {
        self.fx
            .messages()
            .iter()
            .filter(|message| matches!(message, DaemonMessage::ShellsChanged { .. }))
            .count()
    }
}

/// `{ terminal: text }` of one message's activity.
fn texts(activity: &Map<String, Value>) -> Vec<(String, String)> {
    activity
        .iter()
        .map(|(name, line)| (name.clone(), line["text"].as_str().unwrap().to_string()))
        .collect()
}

#[test]
fn sends_the_whole_activity_of_the_task_on_each_change_and_empty_when_it_is_gone() {
    let p = setup();
    let service = p.hooks(None);
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    let stop = hook("Stop", json!({}));

    claude(&service, &task.id, &edit("a.ts"), Some("1"));
    claude(&service, &task.id, &edit("b.ts"), Some("2"));
    claude(&service, &task.id, &stop, Some("2"));
    claude(&service, &task.id, &stop, Some("1"));

    let messages = p.activity_messages();
    let ids: Vec<&str> = messages.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, [task.id.as_str(); 4]);
    let all: Vec<Vec<(String, String)>> = messages.iter().map(|(_, a)| texts(a)).collect();
    assert_eq!(
        all,
        [
            pairs(&[("1", "Edit a.ts")]),
            pairs(&[("1", "Edit a.ts"), ("2", "Edit b.ts")]),
            pairs(&[("1", "Edit a.ts")]),
            pairs(&[]),
        ]
    );
}

#[test]
fn sends_nothing_when_the_activity_did_not_change() {
    let p = setup();
    let service = p.hooks(None);
    let task = p.fx.make("Fix it");

    // No line to clear, and no line to show.
    step(&service, &task.id, "Stop");
    step(&service, &task.id, "SessionEnd");

    assert!(p.activity_messages().is_empty());
}

#[test]
fn sends_the_cleared_activity_when_a_turn_ends_after_a_report() {
    let p = setup();
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    let service = p.hooks(Some(mock(|task, _| own_lane(task), |_, _| true)));
    claude(&service, &task.id, &edit("a.ts"), Some("1"));

    claude(&service, &task.id, &hook("Stop", json!({})), Some("1"));

    assert_eq!(
        p.fx.messages().last(),
        Some(&DaemonMessage::ActivityChanged {
            task_id: task.id.clone(),
            activity: Map::new(),
        })
    );
}

#[test]
fn sends_the_cleared_activity_when_a_hook_comes_after_the_workspace_was_removed() {
    let p = setup();
    let service = p.hooks(None);
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    claude(&service, &task.id, &edit("a.ts"), Some("1"));
    p.fx.workspace(&task.id, None);

    claude(&service, &task.id, &edit("b.ts"), Some("1"));
    claude(&service, &task.id, &edit("c.ts"), Some("1"));

    assert_eq!(p.activity_messages()[1..], [(task.id.clone(), Map::new())]);
}

#[test]
fn tells_that_the_workspace_may_have_changed_after_a_tool_ran_and_at_the_end_of_a_turn() {
    let p = setup();
    let service = p.hooks(None);
    let task = p.fx.make("Fix it");

    claude(&service, &task.id, &edit("a.ts"), None);
    step(&service, &task.id, "PostToolUse");
    step(&service, &task.id, "Stop");
    step(&service, &task.id, "StopFailure");
    let elsewhere = hook("Stop", json!({ "cwd": "/elsewhere" }));
    service
        .handle("claude-code", &elsewhere, None, None)
        .unwrap();

    assert_eq!(
        p.touched(),
        [task.id.clone(), task.id.clone(), task.id.clone()]
    );
}

#[test]
fn does_not_tell_for_a_hook_that_changes_no_file() {
    let cases = [
        ("claude-code", hook("UserPromptSubmit", json!({}))),
        (
            "claude-code",
            hook("PreToolUse", json!({ "tool_name": "Bash" })),
        ),
        ("claude-code", hook("PermissionRequest", json!({}))),
        (
            "claude-code",
            hook(
                "Notification",
                json!({ "notification_type": "permission_prompt" }),
            ),
        ),
        ("cursor", hook("preToolUse", json!({}))),
        (
            "cursor",
            hook("beforeShellExecution", json!({ "command": "ls" })),
        ),
        (
            "gemini",
            hook("BeforeTool", json!({ "tool_name": "read_file" })),
        ),
    ];
    for (agent, raw) in cases {
        let p = setup();
        let service = p.hooks(None);
        let task = p.fx.make("Fix it");

        service.handle(agent, &raw, Some(&task.id), None).unwrap();

        assert!(p.touched().is_empty(), "{agent}: {raw}");
    }
}

#[test]
fn names_the_session_of_the_agent_in_each_natamux_shell() {
    let p = setup();
    let sessions: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    let record = Arc::clone(&sessions);
    let mut options = p.fx.options(true);
    options.session_seen = Some(Arc::new(
        move |task: &str, terminal: &str, agent: &str, session: &str| {
            let seen = [task, terminal, agent, session].map(String::from).to_vec();
            record.lock().unwrap().push(seen);
        },
    ));
    let service = p.fx.service_with(Arc::new(NoStages), options);
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    let with_session = |path: &str, id: &str| {
        let mut raw = edit(path);
        raw["session_id"] = json!(id);
        raw
    };

    claude(&service, &task.id, &with_session("a.ts", "a-1"), Some("1"));
    let stop = json!({ "hook_event_name": "stop", "conversation_id": "u-1" });
    service
        .handle("cursor", &stop, Some(&task.id), Some("7"))
        .unwrap();
    // No session, or an agent outside Natamux shells: nothing to restore.
    claude(&service, &task.id, &edit("b.ts"), Some("1"));
    claude(&service, &task.id, &with_session("c.ts", "a-2"), None);

    let id = task.id.as_str();
    assert_eq!(
        *sessions.lock().unwrap(),
        [
            [id, "1", "claude-code", "a-1"].map(String::from).to_vec(),
            [id, "7", "cursor", "u-1"].map(String::from).to_vec(),
        ]
    );
}

type UsageCall = (String, Option<String>, String, String);

#[test]
fn names_the_usage_file_after_a_turn_with_the_stage_the_turn_counts_for() {
    let p = setup();
    let seen: Arc<Mutex<Vec<UsageCall>>> = Arc::default();
    let record = Arc::clone(&seen);
    let stage = Arc::new(Mutex::new("implement".to_string()));
    let reported = Arc::new(AtomicBool::new(false));
    let (lane_stage, take) = (Arc::clone(&stage), Arc::clone(&reported));
    let stages = mock(
        move |task, _| Some((lane_stage.lock().unwrap().clone(), task.state)),
        move |_, _| take.load(Ordering::SeqCst),
    );
    let mut options = p.fx.options(true);
    options.usage_seen = Some(Arc::new(
        move |task: &str, stage: Option<&str>, agent: &str, file: &str| {
            let call = (
                task.to_string(),
                stage.map(String::from),
                agent.to_string(),
                file.to_string(),
            );
            record.lock().unwrap().push(call);
        },
    ));
    let service = p.fx.service_with(stages, options);
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    let file = "/home/me/.claude/projects/w/s-1.jsonl";
    let stop = hook("Stop", json!({ "transcript_path": file }));
    let mut tool = edit("a.ts");
    tool["transcript_path"] = json!(file);

    // A tool that is about to run: the file has no finished model call to read yet.
    claude(&service, &task.id, &tool, Some("1"));
    claude(&service, &task.id, &stop, Some("1"));
    // The report moved the task on before its turn ended, so the turn counts for the stage
    // before.
    *stage.lock().unwrap() = "review".to_string();
    reported.store(true, Ordering::SeqCst);
    claude(&service, &task.id, &stop, Some("1"));
    reported.store(false, Ordering::SeqCst);
    claude(&service, &task.id, &stop, Some("1"));
    // Cursor names no usage file.
    let cursor = json!({ "hook_event_name": "stop" });
    service
        .handle("cursor", &cursor, Some(&task.id), Some("2"))
        .unwrap();

    let call = |stage: &str| -> UsageCall {
        (
            task.id.clone(),
            Some(stage.to_string()),
            "claude-code".to_string(),
            file.to_string(),
        )
    };
    assert_eq!(
        *seen.lock().unwrap(),
        [call("implement"), call("implement"), call("review")]
    );
}

#[test]
fn tells_for_a_hook_after_a_tool_ran() {
    let cases = [
        ("cursor", hook("afterFileEdit", json!({}))),
        ("cursor", hook("afterShellExecution", json!({}))),
        (
            "gemini",
            hook("AfterTool", json!({ "tool_name": "write_file" })),
        ),
        ("codex", hook("PostToolUse", json!({}))),
    ];
    for (agent, raw) in cases {
        let p = setup();
        let service = p.hooks(None);
        let task = p.fx.make("Fix it");

        service.handle(agent, &raw, Some(&task.id), None).unwrap();

        assert_eq!(
            p.touched(),
            std::slice::from_ref(&task.id),
            "{agent}: {raw}"
        );
    }
}

fn shell_two_is_no_stage() -> Arc<dyn StageLookup> {
    mock(
        |task, terminal| {
            if terminal == Some("2") {
                None
            } else {
                own_lane(task)
            }
        },
        |_, _| false,
    )
}

#[test]
fn sends_shells_changed_when_a_hook_changes_the_state_of_an_agent_in_a_shell() {
    let p = setup();
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    let service = p.hooks(Some(shell_two_is_no_stage()));
    let send = |raw: Value, terminal: &str| claude(&service, &task.id, &raw, Some(terminal));

    send(hook("UserPromptSubmit", json!({})), "2");
    assert_eq!(p.shells_changed(), 1);

    // Still running: nothing new to show.
    send(edit("a.ts"), "2");
    send(hook("PostToolUse", json!({})), "2");
    assert_eq!(p.shells_changed(), 1);

    send(hook("Stop", json!({})), "2");
    assert_eq!(p.shells_changed(), 2);

    // The agent of a stage shows the state of the task, which comes with the task.
    send(hook("UserPromptSubmit", json!({})), "1");
    assert_eq!(p.shells_changed(), 2);
}

#[test]
fn sends_no_shells_changed_for_a_shell_whose_agent_the_read_of_the_shells_did_not_see() {
    let p = setup();
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));
    let service = p.hooks(Some(mock(|_, _| None, |_, _| false)));
    let prompt = hook("UserPromptSubmit", json!({}));
    claude(&service, &task.id, &prompt, Some("2"));
    assert_eq!(p.shells_changed(), 1);

    // Each read of the shells drops the state, as no agent shows there (a shell on Windows).
    for _ in 0..3 {
        service.forget_shells_except(&task.id, &[]);
        claude(&service, &task.id, &prompt, Some("2"));
    }
    assert_eq!(p.shells_changed(), 1);
    assert_eq!(service.shell_state(&task.id, "2"), Some(TaskState::Running));

    // Once a read shows an agent there, its next state is sent again.
    service.forget_shells_except(&task.id, &["2".to_string()]);
    claude(&service, &task.id, &hook("Stop", json!({})), Some("2"));
    assert_eq!(p.shells_changed(), 2);
}

#[test]
fn sends_nothing_for_a_hook_of_no_known_task() {
    let p = setup();
    let service = p.hooks(None);

    claude(&service, "T-0099", &edit("a.ts"), None);

    assert!(p.activity_messages().is_empty());
    assert!(p.touched().is_empty());
}

#[test]
fn sends_the_same_line_again_for_the_same_hook_which_the_dashboard_just_sets_again() {
    let p = setup();
    let service = p.hooks(None);
    let task = p.fx.make("Fix it");
    p.fx.workspace(&task.id, Some("/work/T-0001"));

    claude(&service, &task.id, &edit("a.ts"), Some("1"));
    claude(&service, &task.id, &edit("a.ts"), Some("1"));

    let lines: Vec<Vec<(String, String)>> = p
        .activity_messages()
        .iter()
        .map(|(_, activity)| texts(activity))
        .collect();
    assert_eq!(
        lines,
        [pairs(&[("1", "Edit a.ts")]), pairs(&[("1", "Edit a.ts")])]
    );
}
