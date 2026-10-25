//! Ports of `packages/cli/src/__tests__/command.test.ts`, one test per case.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use natamux_cli::command::{Command, Context, NewTask, TaskChanges, UsageError, parse_command};

fn parse(argv: &[&str]) -> Result<Command, UsageError> {
    let argv: Vec<String> = argv.iter().map(|arg| arg.to_string()).collect();
    parse_command(&argv)
}

fn ok(argv: &[&str]) -> Command {
    parse(argv).unwrap()
}

/// The message of the usage error.
fn fails(argv: &[&str]) -> String {
    parse(argv).unwrap_err().0
}

fn fails_with(argv: &[&str], text: &str) {
    let message = fails(argv);
    assert!(
        message.contains(text),
        "{message:?} does not contain {text:?}"
    );
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn text(value: &str) -> Option<String> {
    Some(value.to_string())
}

#[test]
fn shows_help() {
    let cases: [&[&str]; 4] = [&[], &["help"], &["task", "list", "--help"], &["-h"]];
    for argv in cases {
        assert_eq!(ok(argv), Command::Help, "{argv:?}");
    }
}

// task create

#[test]
fn joins_the_words_of_the_title() {
    let Command::TaskCreate(NewTask {
        title,
        labels,
        depends_on,
        json,
        ..
    }) = ok(&["task", "create", "Fix", "the", "bug"])
    else {
        panic!("not task.create");
    };
    assert_eq!(title, "Fix the bug");
    assert!(labels.is_empty());
    assert!(depends_on.is_empty());
    assert!(!json);
}

#[test]
fn reads_every_option() {
    let command = ok(&[
        "task",
        "create",
        "Fix the bug",
        "--description",
        "It is slow",
        "--priority",
        "high",
        "--label",
        "bug",
        "--label",
        "perf",
        "--agent",
        "claude-code",
        "--depends-on",
        "T-1",
        "--depends-on",
        "T-2",
        "--json",
    ]);
    assert_eq!(
        command,
        Command::TaskCreate(NewTask {
            title: "Fix the bug".into(),
            description: text("It is slow"),
            priority: text("high"),
            labels: strings(&["bug", "perf"]),
            agent: text("claude-code"),
            depends_on: strings(&["T-1", "T-2"]),
            context: None,
            attach: None,
            repo: None,
            parent_id: None,
            json: true,
        })
    );
}

#[test]
fn reads_parent_as_parent_id_and_rejects_a_bad_id() {
    let Command::TaskCreate(NewTask { parent_id, .. }) =
        ok(&["task", "create", "x", "--parent", "T-0001"])
    else {
        panic!("not task.create");
    };
    assert_eq!(parent_id, text("T-0001"));
    fails_with(
        &["task", "create", "x", "--parent", "nope"],
        "not a task id",
    );
}

#[test]
fn needs_a_title() {
    assert!(parse(&["task", "create"]).is_err());
}

#[test]
fn splits_context_on_the_first_colon_only_and_sends_no_skill() {
    // `Context` has no skill field, so a skill cannot be sent.
    let Command::TaskCreate(NewTask { context, .. }) = ok(&[
        "task",
        "create",
        "x",
        "--context",
        "skill:ticket-reader:REC-1",
    ]) else {
        panic!("not task.create");
    };
    assert_eq!(
        context,
        Some(Context {
            source: "skill".into(),
            reference: "ticket-reader:REC-1".into(),
        })
    );
}

#[test]
fn reads_the_project_from_repo_and_checks_it_is_an_id() {
    let Command::TaskCreate(NewTask { repo, .. }) =
        ok(&["task", "create", "x", "--repo", "my-app"])
    else {
        panic!("not task.create");
    };
    assert_eq!(repo, text("my-app"));
    let Command::TaskCreate(NewTask { repo, .. }) = ok(&["task", "create", "x"]) else {
        panic!("not task.create");
    };
    assert_eq!(repo, None);
    fails_with(
        &["task", "create", "x", "--repo", "My App"],
        "not a project id",
    );
}

#[test]
fn reads_each_attach() {
    let argv = [
        "task", "create", "x", "--attach", "a.pdf", "--attach", "b.png",
    ];
    let Command::TaskCreate(NewTask { attach, .. }) = ok(&argv) else {
        panic!("not task.create");
    };
    assert_eq!(attach, Some(strings(&["a.pdf", "b.png"])));
    let Command::TaskCreate(NewTask { attach, .. }) = ok(&["task", "create", "x"]) else {
        panic!("not task.create");
    };
    assert_eq!(attach, None);
}

#[test]
fn rejects_an_unknown_priority() {
    fails_with(
        &["task", "create", "x", "--priority", "asap"],
        "--priority must be one of",
    );
}

#[test]
fn rejects_a_dependency_that_is_not_a_task_id() {
    fails_with(
        &["task", "create", "x", "--depends-on", "nope"],
        "not a task id",
    );
}

// task list and show

#[test]
fn lists_all_tasks() {
    assert_eq!(
        ok(&["task", "list"]),
        Command::TaskList {
            state: None,
            json: false,
        }
    );
}

#[test]
fn filters_by_state() {
    let Command::TaskList { state, .. } = ok(&["task", "list", "--state", "review"]) else {
        panic!("not task.list");
    };
    assert_eq!(state, text("review"));
}

#[test]
fn rejects_an_unknown_state() {
    fails_with(
        &["task", "list", "--state", "finished"],
        "--state must be one of",
    );
}

#[test]
fn rejects_extra_arguments_after_list() {
    fails_with(&["task", "list", "T-1"], "Unexpected argument");
}

#[test]
fn shows_one_task() {
    assert_eq!(
        ok(&["task", "show", "T-42", "--json"]),
        Command::TaskShow {
            id: text("T-42"),
            json: true,
        }
    );
}

#[test]
fn shows_the_task_of_the_terminal_without_an_id_and_needs_a_valid_id() {
    assert_eq!(
        ok(&["task", "show"]),
        Command::TaskShow {
            id: None,
            json: false,
        }
    );
    fails_with(&["task", "show", "42"], "not a task id");
}

#[test]
fn shows_a_task_with_an_id_the_user_chose_and_names_both_id_forms_on_a_bad_id() {
    let Command::TaskShow { id, .. } = ok(&["task", "show", "TASK-0005"]) else {
        panic!("not task.show");
    };
    assert_eq!(id, text("TASK-0005"));
    fails_with(&["task", "show", "task-0005"], "T-42 or ABC-42");
}

#[test]
fn rejects_an_unknown_task_command() {
    fails_with(&["task", "remove", "T-1"], "Unknown task command");
}

// event

#[test]
fn sends_an_event_as_a_human_with_an_empty_payload_by_default() {
    assert_eq!(
        ok(&["event", "T-1", "task.readied"]),
        Command::EventSend {
            id: "T-1".into(),
            kind: "task.readied".into(),
            source: "human".into(),
            payload: serde_json::Map::new(),
            json: false,
        }
    );
}

#[test]
fn reads_the_source_and_the_payload() {
    let Command::EventSend {
        source, payload, ..
    } = ok(&[
        "event",
        "T-1",
        "state.overridden",
        "--source",
        "agent",
        "--payload",
        r#"{"to":"ready"}"#,
    ])
    else {
        panic!("not event.send");
    };
    assert_eq!(source, "agent");
    assert_eq!(
        serde_json::Value::Object(payload),
        serde_json::json!({"to": "ready"})
    );
}

#[test]
fn needs_an_id_and_an_event_type() {
    fails_with(&["event"], "task id is required");
    fails_with(&["event", "T-1"], "event type is required");
}

#[test]
fn rejects_an_unknown_event_type_and_source() {
    fails_with(
        &["event", "T-1", "agent.exploded"],
        "event type must be one of",
    );
    fails_with(
        &["event", "T-1", "task.readied", "--source", "robot"],
        "--source must be one of",
    );
}

#[test]
fn rejects_a_payload_that_is_not_a_json_object() {
    fails_with(
        &["event", "T-1", "task.readied", "--payload", "{oops"],
        "valid JSON",
    );
    fails_with(
        &["event", "T-1", "task.readied", "--payload", "[1]"],
        "JSON object",
    );
    fails_with(
        &["event", "T-1", "task.readied", "--payload", "null"],
        "JSON object",
    );
}

// workspace

#[test]
fn creates_a_workspace() {
    assert_eq!(
        ok(&["workspace", "create", "T-1"]),
        Command::WorkspaceCreate {
            id: "T-1".into(),
            base: None,
            json: false,
        }
    );
}

#[test]
fn creates_a_workspace_from_a_branch() {
    assert_eq!(
        ok(&["workspace", "create", "T-1", "--base", "release"]),
        Command::WorkspaceCreate {
            id: "T-1".into(),
            base: text("release"),
            json: false,
        }
    );
}

#[test]
fn removes_a_workspace_only_with_yes() {
    fails_with(&["workspace", "remove", "T-1"], "--yes");
    assert_eq!(
        ok(&["workspace", "remove", "T-1", "--yes"]),
        Command::WorkspaceRemove {
            id: "T-1".into(),
            delete_branch: false,
            json: false,
        }
    );
}

#[test]
fn reads_delete_branch() {
    let Command::WorkspaceRemove { delete_branch, .. } =
        ok(&["workspace", "remove", "T-1", "--yes", "--delete-branch"])
    else {
        panic!("not workspace.remove");
    };
    assert!(delete_branch);
}

#[test]
fn needs_a_valid_task_id_and_a_known_command() {
    fails_with(&["workspace", "create"], "task id is required");
    fails_with(&["workspace", "create", "nope"], "not a task id");
    fails_with(&["workspace", "list"], "Unknown workspace command");
}

// hooks

#[test]
fn prints_the_claude_code_settings() {
    assert_eq!(ok(&["hooks", "claude-code"]), Command::HooksClaudeCode);
}

#[test]
fn rejects_an_unknown_tool_and_extra_arguments() {
    fails_with(&["hooks"], "Unknown hooks command");
    fails_with(&["hooks", "cursor"], "Unknown hooks command");
    fails_with(&["hooks", "claude-code", "extra"], "Unexpected argument");
}

#[test]
fn does_not_let_a_user_send_an_event_that_only_the_daemon_records() {
    fails_with(
        &["event", "T-1", "workspace.created"],
        "event type must be one of",
    );
}

#[test]
fn shows_the_event_log_of_a_task() {
    assert_eq!(
        ok(&["events", "T-1"]),
        Command::EventList {
            id: "T-1".into(),
            json: false,
        }
    );
}

#[test]
fn rejects_an_unknown_command_and_an_unknown_option() {
    fails_with(&["deploy"], "Unknown command \"deploy\"");
    assert!(parse(&["task", "list", "--colour"]).is_err());
}

// more commands

#[test]
fn reads_task_edit_with_only_the_options_given() {
    assert_eq!(
        ok(&[
            "task", "edit", "T-1", "--title", "New", "--label", "a", "--label", "b"
        ]),
        Command::TaskEdit {
            id: "T-1".into(),
            changes: TaskChanges {
                title: text("New"),
                labels: Some(strings(&["a", "b"])),
                ..TaskChanges::default()
            },
            json: false,
        }
    );
    assert!(parse(&["task", "edit", "T-1"]).is_err());
    assert!(parse(&["task", "edit", "T-1", "--priority", "someday"]).is_err());
}

#[test]
fn reads_agent_start_with_claude_code_as_the_default_agent() {
    assert_eq!(
        ok(&["agent", "start", "T-2", "--prompt", "Go"]),
        Command::AgentStart {
            id: "T-2".into(),
            agent: "claude-code".into(),
            prompt: text("Go"),
            base: None,
            json: false,
        }
    );
    let Command::AgentStart { base, .. } = ok(&["agent", "start", "T-2", "--base", "develop"])
    else {
        panic!("not agent.start");
    };
    assert_eq!(base, text("develop"));
    assert!(parse(&["agent", "stop", "T-2"]).is_err());
}

#[test]
fn reads_ask_and_done_with_or_without_task() {
    assert_eq!(
        ok(&["ask", "Which", "one?"]),
        Command::AgentAsk {
            id: None,
            question: "Which one?".into(),
            stage: None,
            json: false,
        }
    );
    assert_eq!(
        ok(&["done", "--task", "T-3"]),
        Command::AgentDone {
            id: text("T-3"),
            stage: None,
            json: false,
        }
    );
    assert!(parse(&["ask"]).is_err());
    assert!(parse(&["done", "extra"]).is_err());
}

#[test]
fn reads_the_stage_of_ask_and_done_and_refuses_stage_for_stage_back() {
    let Command::AgentDone { stage, .. } = ok(&["done", "--stage", "test-spec"]) else {
        panic!("not agent.done");
    };
    assert_eq!(stage, text("test-spec"));
    let Command::AgentAsk { stage, .. } = ok(&["ask", "Which?", "--stage", "implement"]) else {
        panic!("not agent.ask");
    };
    assert_eq!(stage, text("implement"));
    fails_with(&["stage", "back", "T-1", "--stage", "plan"], "--to");
}

#[test]
fn reads_stage_workflow_with_its_stage_and_on_or_off() {
    assert_eq!(
        ok(&["stage", "workflow", "T-1", "on", "--stage", "implement"]),
        Command::StageWorkflow {
            id: "T-1".into(),
            stage: "implement".into(),
            on: true,
            json: false,
        }
    );
    assert_eq!(
        ok(&[
            "stage", "workflow", "T-1", "off", "--stage", "plan", "--json"
        ]),
        Command::StageWorkflow {
            id: "T-1".into(),
            stage: "plan".into(),
            on: false,
            json: true,
        }
    );
    fails_with(&["stage", "workflow", "T-1", "on"], "--stage");
    fails_with(
        &["stage", "workflow", "T-1", "--stage", "plan"],
        "on or off",
    );
    fails_with(
        &["stage", "workflow", "T-1", "yes", "--stage", "plan"],
        "on or off",
    );
    assert!(parse(&["stage", "workflow", "T-1", "on", "more", "--stage", "plan"]).is_err());
}

#[test]
fn reads_stage_ci_with_its_stage_and_on_or_off() {
    assert_eq!(
        ok(&["stage", "ci", "T-1", "on", "--stage", "implement"]),
        Command::StageCi {
            id: "T-1".into(),
            stage: "implement".into(),
            on: true,
            json: false,
        }
    );
    fails_with(&["stage", "ci", "T-1", "on"], "--stage");
    fails_with(
        &["stage", "ci", "T-1", "maybe", "--stage", "testing"],
        "natamux stage ci T-42 on --stage implement",
    );
}
