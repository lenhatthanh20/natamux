//! Ports of `packages/cli/src/__tests__/run.test.ts` and the `hooks claude-code` cases of
//! `hooks.test.ts`, one test per case.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use natamux_cli::run::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, Env};
use serde_json::{Value, json};
use support::*;

fn body_of(call: &Call) -> Value {
    call.json().unwrap()
}

// help and usage

#[test]
fn prints_help_without_a_token_and_without_calling_the_daemon() {
    let result = run_plain(&["help"], Env::default());
    assert_eq!(result.code, EXIT_OK);
    assert!(result.stdout.contains("Usage: natamux"));
    assert!(result.stdout.contains("agent.started"));
    assert!(result.calls.is_empty());
}

#[test]
fn exits_with_2_and_a_hint_for_a_wrong_command_line() {
    let result = run(&["task", "create"], |_| json_reply(json!({})));
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.stderr.contains("A title is required"));
    assert!(result.stderr.contains("natamux help"));
    assert!(result.calls.is_empty());
}

#[test]
fn exits_with_2_when_the_token_is_missing() {
    let result = run_plain(&["task", "list"], Env::default());
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.stderr.contains("NATAMUX_TOKEN"));
    assert!(result.calls.is_empty());
}

// requests

#[test]
fn sends_the_token_and_uses_the_default_address() {
    let result = run(&["task", "list"], |_| json_reply(json!({"tasks": []})));
    let call = &result.calls[0];
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks");
    assert_eq!(call.method, "GET");
    assert_eq!(call.token, "secret");
}

#[test]
fn uses_natamux_url_and_drops_a_trailing_slash() {
    let result = run_with(
        &["task", "list"],
        Env::from([
            ("NATAMUX_TOKEN", "secret"),
            ("NATAMUX_URL", "http://example.test:9000/"),
        ]),
        b"",
        |_| json_reply(json!({"tasks": []})),
    );
    assert_eq!(result.calls[0].url, "http://example.test:9000/tasks");
}

// task create

#[test]
fn sends_the_task_and_prints_its_id() {
    let result = run(
        &[
            "task",
            "create",
            "Fix the dashboard",
            "--priority",
            "high",
            "--label",
            "ui",
            "--depends-on",
            "T-0007",
            "--repo",
            "web",
        ],
        |_| task_created(&make_task()),
    );

    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "Created T-0001  Fix the dashboard\n");
    assert_eq!(result.calls.len(), 1);
    let call = &result.calls[0];
    assert_eq!(call.header("content-type"), Some("application/json"));
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks");
    assert_eq!(call.method, "POST");
    assert_has(
        &body_of(call),
        &json!({
            "title": "Fix the dashboard",
            "priority": "high",
            "labels": ["ui"],
            "dependsOn": ["T-0007"],
            "repo": "web",
        }),
    );
}

#[test]
fn sends_a_title_that_starts_with_an_id_as_typed_and_prints_the_chosen_id() {
    let task = make_task_with(|task| {
        task.id = "TASK-0006".into();
        task.title = "x".into();
    });
    let result = run(
        &["task", "create", "Task-0006: x", "--repo", "app"],
        move |_| task_created(&task),
    );
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "Created TASK-0006  x\n");
    assert_has(
        &body_of(&result.calls[0]),
        &json!({"title": "Task-0006: x"}),
    );
}

#[test]
fn prints_json_with_json() {
    let task = make_task();
    let expected = serde_json::to_value(&task).unwrap();
    let result = run(
        &["task", "create", "Fix the dashboard", "--json"],
        one_project(move |_| task_created(&task)),
    );
    let printed: Value = serde_json::from_str(&result.stdout).unwrap();
    assert_eq!(printed, expected);
}

#[test]
fn puts_the_task_in_the_only_project_when_none_is_named() {
    let result = run(
        &["task", "create", "Fix the dashboard"],
        one_project(|_| task_created(&make_task())),
    );
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(
        result.lines(),
        [
            "GET http://127.0.0.1:4317/repos",
            "POST http://127.0.0.1:4317/tasks"
        ]
    );
    assert_has(&body_of(&result.calls[1]), &json!({"repo": "app"}));
}

#[test]
fn puts_the_task_in_the_project_of_the_terminal_from_natamux_task_id() {
    let result = run_with(
        &["task", "create", "Follow-up"],
        Env::from([("NATAMUX_TOKEN", "secret"), ("NATAMUX_TASK_ID", "T-7")]),
        b"",
        |_| {
            task_reply(&make_task_with(|task| {
                task.id = "T-7".into();
                task.repo = Some("web".into());
            }))
        },
    );
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.calls[0].url, "http://127.0.0.1:4317/tasks/T-7");
    assert_eq!(result.calls[1].method, "POST");
    assert_has(&body_of(&result.calls[1]), &json!({"repo": "web"}));
}

#[test]
fn sends_parent_id_and_no_repo_with_parent_and_no_repo() {
    let result = run(&["task", "create", "Part", "--parent", "T-0001"], |_| {
        task_created(&make_task())
    });
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.calls.len(), 1);
    let body = body_of(&result.calls[0]);
    assert_has(&body, &json!({"parentId": "T-0001"}));
    assert!(body.get("repo").is_none());
}

#[test]
fn still_sends_the_repo_with_parent_and_repo() {
    let result = run(
        &[
            "task", "create", "Part", "--parent", "T-0001", "--repo", "web",
        ],
        |_| task_created(&make_task()),
    );
    assert_has(
        &body_of(&result.calls[0]),
        &json!({"parentId": "T-0001", "repo": "web"}),
    );
}

#[test]
fn asks_for_repo_and_lists_the_projects_when_there_are_several() {
    let result = run(&["task", "create", "Fix the dashboard"], |_| {
        json_reply(json!({
            "repos": [
                {"id": "app", "name": "app", "path": "/work/app"},
                {"id": "web", "name": "web", "path": "/work/web"},
            ]
        }))
    });
    assert_eq!(result.code, EXIT_FAILED);
    assert_eq!(
        result.stderr,
        "Error: Name the project with --repo. Projects: app, web\n"
    );
    let methods: Vec<&str> = result
        .calls
        .iter()
        .map(|call| call.method.as_str())
        .collect();
    assert_eq!(methods, ["GET"]);
}

// task create with --attach

#[test]
fn sends_each_file_by_its_name_as_base64() {
    let dir = TempDir::new("attach");
    std::fs::write(dir.join("spec.pdf"), "pdf").unwrap();
    std::fs::write(dir.join("shot.png"), [0x89, 0x50, 0x4e, 0x47]).unwrap();
    let (spec, shot) = (dir.join("spec.pdf"), dir.join("shot.png"));

    let result = run(
        &[
            "task", "create", "Fix it", "--repo", "web", "--attach", &spec, "--attach", &shot,
        ],
        |_| task_created(&make_task()),
    );

    assert_eq!(result.code, EXIT_OK);
    assert_has(
        &body_of(&result.calls[0]),
        &json!({"attachments": [
            {"name": "spec.pdf", "data": STANDARD.encode("pdf")},
            {"name": "shot.png", "data": STANDARD.encode([0x89, 0x50, 0x4e, 0x47])},
        ]}),
    );
}

#[test]
fn sends_no_attachments_without_attach() {
    let result = run(&["task", "create", "Fix it", "--repo", "web"], |_| {
        task_created(&make_task())
    });
    assert!(body_of(&result.calls[0]).get("attachments").is_none());
}

#[test]
fn makes_no_task_when_a_file_is_missing() {
    let dir = TempDir::new("attach");
    let missing = dir.join("missing.pdf");
    let result = run(
        &[
            "task", "create", "Fix it", "--repo", "web", "--attach", &missing,
        ],
        |_| json_reply(json!({})),
    );
    assert_eq!(result.code, EXIT_FAILED);
    assert_eq!(
        result.stderr,
        format!("Error: There is no file at {missing}\n")
    );
    assert!(result.calls.is_empty());
}

#[test]
fn makes_no_task_when_a_file_is_over_the_limit() {
    let dir = TempDir::new("attach");
    let big = dir.join("big.mov");
    // A sparse file: no 20 MB is written.
    std::fs::File::create(&big)
        .unwrap()
        .set_len(20_000_001)
        .unwrap();
    let result = run(
        &[
            "task", "create", "Fix it", "--repo", "web", "--attach", &big,
        ],
        |_| json_reply(json!({})),
    );
    assert_eq!(result.code, EXIT_FAILED);
    assert!(
        result
            .stderr
            .contains("is larger than 20 MB, the most for one file"),
        "{}",
        result.stderr
    );
    assert!(result.calls.is_empty());
}

// task list

#[test]
fn prints_a_table() {
    let result = run(&["task", "list"], |_| {
        json_reply(json!({"tasks": [
            make_task_with(|task| {
                task.id = "T-0002".into();
                task.title = "Second".into();
                task.state = natamux_proto::tasks::task::TaskState::Review;
                task.priority = "high".into();
            }),
            make_task_with(|task| {
                task.id = "T-0001".into();
                task.title = "First".into();
            }),
        ]}))
    });
    assert_eq!(
        cells(&result.stdout),
        [
            ["ID", "STATE", "PRIORITY", "TITLE"],
            ["T-0002", "review", "high", "Second"],
            ["T-0001", "draft", "normal", "First"],
        ]
    );
}

#[test]
fn says_so_when_there_are_no_tasks() {
    let result = run(&["task", "list"], |_| json_reply(json!({"tasks": []})));
    assert_eq!(result.stdout, "No tasks.\n");
}

#[test]
fn sends_the_state_filter() {
    let result = run(&["task", "list", "--state", "review"], |_| {
        json_reply(json!({"tasks": []}))
    });
    assert_eq!(
        result.calls[0].url,
        "http://127.0.0.1:4317/tasks?state=review"
    );
}

// task show

#[test]
fn prints_the_fields_and_the_description() {
    let result = run(&["task", "show", "T-0001"], |_| {
        task_reply(&make_task_with(|task| {
            task.description = "It is slow.".into();
            task.labels = vec!["ui".into(), "perf".into()];
            task.agent = Some("claude".into());
        }))
    });
    assert_eq!(result.calls[0].url, "http://127.0.0.1:4317/tasks/T-0001");
    for text in [
        "T-0001  Fix the dashboard",
        "Labels:     ui, perf",
        "Agent:      claude",
        "Branch:     -",
        "It is slow.",
    ] {
        assert!(
            result.stdout.contains(text),
            "{text:?} not in {}",
            result.stdout
        );
    }
}

#[test]
fn shows_the_task_of_the_terminal_from_natamux_task_id() {
    let result = run_with(
        &["task", "show"],
        Env::from([("NATAMUX_TOKEN", "secret"), ("NATAMUX_TASK_ID", "T-7")]),
        b"",
        |_| task_reply(&make_task_with(|task| task.id = "T-7".into())),
    );
    assert_eq!(result.code, 0);
    assert_eq!(result.calls[0].url, "http://127.0.0.1:4317/tasks/T-7");
}

#[test]
fn needs_an_id_outside_a_task_terminal() {
    let result = run(&["task", "show"], |_| json_reply(json!({})));
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("natamux task show T-42"));
    assert!(result.calls.is_empty());
}

// event

#[test]
fn sends_the_event_and_reports_that_it_was_accepted() {
    let result = run(&["event", "T-0001", "task.readied"], |_| {
        json_reply(json!({
            "task": make_task_with(|task| task.state = natamux_proto::tasks::task::TaskState::Ready),
            "accepted": true,
        }))
    });
    assert_eq!(result.stdout, "Accepted. T-0001 is now ready.\n");
    let call = &result.calls[0];
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-0001/events");
    assert_eq!(call.method, "POST");
    assert_has(
        &body_of(call),
        &json!({"source": "human", "type": "task.readied", "payload": {}}),
    );
}

#[test]
fn reports_an_event_that_was_not_accepted() {
    let result = run(
        &["event", "T-0001", "agent.finished", "--source", "agent"],
        |_| json_reply(json!({"task": make_task(), "accepted": false})),
    );
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.stdout, "Not accepted. T-0001 stays in draft.\n");
}

// events

#[test]
fn prints_the_event_log() {
    let result = run(&["events", "T-0001"], |_| {
        json_reply(json!({"events": [
            {"seq": 1, "accepted": true, "event": {
                "taskId": "T-0001", "source": "system", "type": "task.created",
                "at": "2026-10-08T09:00:00.000Z", "payload": {},
            }},
            {"seq": 2, "accepted": false, "event": {
                "taskId": "T-0001", "source": "agent", "type": "agent.finished",
                "at": "2026-10-08T09:01:00.000Z", "payload": {},
            }},
        ]}))
    });
    assert_eq!(
        cells(&result.stdout),
        [
            vec!["SEQ", "AT", "SOURCE", "TYPE", "ACCEPTED"],
            vec![
                "1",
                "2026-10-08T09:00:00.000Z",
                "system",
                "task.created",
                "yes"
            ],
            vec![
                "2",
                "2026-10-08T09:01:00.000Z",
                "agent",
                "agent.finished",
                "no"
            ],
        ]
    );
}

// workspace

fn with_workspace() -> natamux_proto::tasks::task::Task {
    make_task_with(|task| {
        task.workspace = Some("/work/.natamux/worktrees/T-0001".into());
        task.branch = Some("natamux/t-0001-fix-the-dashboard".into());
    })
}

#[test]
fn creates_a_workspace_and_prints_its_folder_and_branch() {
    let result = run(&["workspace", "create", "T-0001"], |_| {
        task_created(&with_workspace())
    });
    let call = &result.calls[0];
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-0001/workspace");
    assert_eq!(call.method, "POST");
    // No body, so no JSON content type: the daemon answers 400 to that.
    assert_eq!(call.header("content-type"), None);
    assert!(result.stdout.contains("Created the workspace of T-0001"));
    assert!(result.stdout.contains("natamux/t-0001-fix-the-dashboard"));
    assert!(result.stdout.contains("/work/.natamux/worktrees/T-0001"));
}

#[test]
fn removes_a_workspace_and_says_whether_the_branch_was_deleted() {
    let result = run(
        &["workspace", "remove", "T-0001", "--yes", "--delete-branch"],
        |_| {
            json_reply(json!({
                "task": make_task_with(|task| {
                    task.workspace = None;
                    task.branch = None;
                }),
                "branchDeleted": true,
            }))
        },
    );
    let call = &result.calls[0];
    assert_eq!(
        call.url,
        "http://127.0.0.1:4317/tasks/T-0001/workspace?deleteBranch=true"
    );
    assert_eq!(call.method, "DELETE");
    assert_eq!(call.header("content-type"), None);
    assert_eq!(
        result.stdout,
        "Removed the workspace of T-0001. The branch was deleted.\n"
    );
}

#[test]
fn says_when_the_branch_is_kept() {
    let result = run(&["workspace", "remove", "T-0001", "--yes"], |_| {
        json_reply(json!({
            "task": make_task_with(|task| {
                task.workspace = None;
                task.branch = Some("natamux/t-0001-fix".into());
            }),
            "branchDeleted": false,
        }))
    });
    assert!(result.calls[0].url.contains("deleteBranch=false"));
    assert!(
        result
            .stdout
            .contains("The branch natamux/t-0001-fix is kept.")
    );
}

#[test]
fn does_not_call_the_daemon_without_yes() {
    let result = run(&["workspace", "remove", "T-0001"], |_| {
        json_reply(json!({}))
    });
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.calls.is_empty());
}

#[test]
fn shows_the_reason_when_git_refuses() {
    let result = run(&["workspace", "remove", "T-0001", "--yes"], |_| {
        json_status(
            409,
            json!({"error": "conflict", "message": "git worktree failed: contains modified files"}),
        )
    });
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("contains modified files"));
}

// failures

#[test]
fn shows_the_message_of_a_task_id_that_is_used() {
    let result = run(
        &["task", "create", "Task-0005: Other", "--repo", "app"],
        |_| {
            json_status(
                400,
                json!({"error": "invalid_request", "message": "Task id TASK-0005 is already used"}),
            )
        },
    );
    assert_eq!(result.code, EXIT_FAILED);
    assert_eq!(result.stderr, "Error: Task id TASK-0005 is already used\n");
}

#[test]
fn shows_the_message_of_an_unknown_task() {
    let result = run(&["task", "show", "T-0099"], |_| {
        json_status(
            404,
            json!({"error": "not_found", "message": "Task T-0099 does not exist"}),
        )
    });
    assert_eq!(result.code, EXIT_FAILED);
    assert_eq!(result.stderr, "Error: Task T-0099 does not exist\n");
}

#[test]
fn shows_the_issues_of_an_invalid_request() {
    let result = run(&["task", "create", "x", "--repo", "app"], |_| {
        json_status(
            400,
            json!({
                "error": "invalid_request",
                "issues": [{"path": ["title"], "message": "Too short"}],
            }),
        )
    });
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("title: Too short"));
}

#[test]
fn tells_the_user_to_check_the_token_when_the_daemon_answers_401() {
    let result = run(&["task", "list"], |_| {
        json_status(401, json!({"error": "unauthorized"}))
    });
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("Check NATAMUX_TOKEN"));
}

#[test]
fn passes_on_why_the_daemon_refused_a_task_terminal_with_403() {
    let message = "Only a person can approve a stage. Use the dashboard.";
    let result = run(&["stage", "approve", "T-1"], move |_| {
        json_status(403, json!({"error": "forbidden", "message": message}))
    });
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains(message));
}

#[test]
fn explains_a_403_with_no_body_or_a_body_that_is_not_json() {
    for text in ["", "Forbidden"] {
        let result = run(&["stage", "approve", "T-1"], move |_| raw_reply(403, text));
        assert_eq!(result.code, EXIT_FAILED, "{text:?}");
        assert!(
            result.stderr.contains("Only a person can do this"),
            "{text:?}: {}",
            result.stderr
        );
    }
}

#[test]
fn says_when_the_daemon_cannot_be_reached() {
    let result = run(&["task", "list"], |_| Reply::Down);
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("Cannot reach the daemon"));
}

#[test]
fn does_not_show_a_stack_for_an_unexpected_answer() {
    let result = run(&["task", "list"], |_| json_reply(json!(["nope"])));
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("Unexpected answer from the daemon"));
}

// more commands

#[test]
fn edits_a_task() {
    let result = run(&["task", "edit", "T-1", "--title", "New title"], |_| {
        task_reply(&make_task_with(|task| {
            task.id = "T-1".into();
            task.title = "New title".into();
        }))
    });
    assert_eq!(result.code, 0);
    let call = &result.calls[0];
    assert_eq!(call.method, "PATCH");
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-1");
    assert_has(&body_of(call), &json!({"title": "New title"}));
    assert!(result.stdout.contains("New title"));
}

fn task_t(id: &str) -> natamux_proto::tasks::task::Task {
    let id = id.to_string();
    make_task_with(|task| task.id = id)
}

#[test]
fn starts_an_agent() {
    let result = run(&["agent", "start", "T-1"], |_| task_reply(&task_t("T-1")));
    let call = &result.calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-1/agent");
    assert_has(&body_of(call), &json!({"agent": "claude-code"}));
    assert_eq!(
        result.stdout,
        "Started claude-code in the terminal of T-1\n"
    );
}

#[test]
fn sends_the_base_branch_for_a_new_workspace_with_agent_start() {
    let result = run(
        &[
            "agent", "start", "T-1", "--agent", "codex", "--base", "develop",
        ],
        |_| task_reply(&task_t("T-1")),
    );
    assert_has(
        &body_of(&result.calls[0]),
        &json!({"agent": "codex", "base": "develop"}),
    );
}

#[test]
fn asks_for_the_task_of_the_terminal_from_natamux_task_id() {
    let result = run_with(
        &["ask", "Which database?"],
        Env::from([("NATAMUX_TOKEN", "secret"), ("NATAMUX_TASK_ID", "T-7")]),
        b"",
        |_| json_reply(json!({"task": task_t("T-7"), "accepted": true})),
    );
    assert_eq!(result.code, 0);
    let call = &result.calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-7/events");
    assert_has(
        &body_of(call),
        &json!({
            "source": "agent",
            "type": "agent.asked",
            "payload": {"question": "Which database?"},
        }),
    );
}

#[test]
fn names_the_terminal_it_runs_in_so_the_daemon_knows_the_stage_inside_a_group() {
    let env = || {
        Env::from([
            ("NATAMUX_TOKEN", "secret"),
            ("NATAMUX_TASK_ID", "T-7"),
            ("NATAMUX_TERMINAL", "2"),
        ])
    };
    let handler = |_: &Call| json_reply(json!({"task": task_t("T-7"), "accepted": true}));

    let ask = run_with(&["ask", "Which file?"], env(), b"", handler);
    assert_eq!(
        body_of(&ask.calls[0]),
        json!({
            "source": "agent",
            "type": "agent.asked",
            "payload": {"question": "Which file?", "terminal": "2"},
        })
    );
    let done = run_with(&["done", "--stage", "test-spec"], env(), b"", handler);
    assert_eq!(
        body_of(&done.calls[0]),
        json!({"action": "report", "stage": "test-spec", "terminal": "2"})
    );
    let approve = run_with(&["stage", "approve", "T-7"], env(), b"", handler);
    assert_eq!(
        body_of(&approve.calls[0]),
        json!({"action": "approve", "terminal": "2"})
    );
    // A terminal of another task says nothing about this one.
    let other = run_with(&["stage", "approve", "T-9"], env(), b"", handler);
    assert_eq!(body_of(&other.calls[0]), json!({"action": "approve"}));
}

#[test]
fn reports_the_stage_done_for_the_task_of_the_terminal_with_a_chosen_id() {
    let result = run_with(
        &["done"],
        Env::from([
            ("NATAMUX_TOKEN", "secret"),
            ("NATAMUX_TASK_ID", "TASK-0005"),
        ]),
        b"",
        |_| task_reply(&task_t("TASK-0005")),
    );
    assert_eq!(result.code, EXIT_OK);
    let call = &result.calls[0];
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/TASK-0005/stage");
    assert_has(&body_of(call), &json!({"action": "report"}));
}

#[test]
fn reports_the_stage_done_for_the_task_named_with_task() {
    let result = run(&["done", "--task", "T-8"], |_| task_reply(&task_t("T-8")));
    let call = &result.calls[0];
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-8/stage");
    assert_has(&body_of(call), &json!({"action": "report"}));
}

#[test]
fn approves_a_stage_and_sends_a_task_back_with_a_reason() {
    let approve = run(&["stage", "approve", "T-8", "--stage", "test-spec"], |_| {
        task_reply(&task_t("T-8"))
    });
    assert_eq!(
        approve.calls[0].url,
        "http://127.0.0.1:4317/tasks/T-8/stage"
    );
    assert_has(
        &body_of(&approve.calls[0]),
        &json!({"action": "approve", "stage": "test-spec"}),
    );
    let back = run(
        &[
            "stage",
            "back",
            "T-8",
            "--to",
            "plan",
            "--reason",
            "Add tests",
        ],
        |_| task_reply(&task_t("T-8")),
    );
    assert_has(
        &body_of(&back.calls[0]),
        &json!({"action": "reopen", "to": "plan", "reason": "Add tests"}),
    );
}

#[test]
fn turns_the_workflow_of_a_stage_on_and_off() {
    let handler = |_: &Call| task_reply(&task_t("T-8"));
    let on = run(
        &["stage", "workflow", "T-8", "on", "--stage", "implement"],
        handler,
    );
    assert_eq!(on.code, 0);
    let call = &on.calls[0];
    assert_eq!(call.method, "PUT");
    assert_eq!(call.url, "http://127.0.0.1:4317/tasks/T-8/workflow");
    assert_has(&body_of(call), &json!({"stage": "implement", "on": true}));
    assert!(
        on.stdout
            .contains("implement runs with a workflow the next time it starts")
    );
    let off = run(
        &["stage", "workflow", "T-8", "off", "--stage", "implement"],
        handler,
    );
    assert_has(
        &body_of(&off.calls[0]),
        &json!({"stage": "implement", "on": false}),
    );
    assert!(off.stdout.contains("implement runs without a workflow"));
}

#[test]
fn links_an_issue_and_creates_a_task_with_its_context_source() {
    let link = run(
        &[
            "task",
            "link",
            "T-8",
            "--issue",
            "ABC-1",
            "--url",
            "https://jira.example.com/ABC-1",
        ],
        |_| task_reply(&task_t("T-8")),
    );
    assert_eq!(link.calls[0].url, "http://127.0.0.1:4317/tasks/T-8/issue");
    assert_has(
        &body_of(&link.calls[0]),
        &json!({"provider": "tracker", "key": "ABC-1"}),
    );
    let create = run(
        &["task", "create", "Export", "--context", "github-issue:42"],
        one_project(|_| task_created(&task_t("T-9"))),
    );
    assert_has(
        &body_of(create.calls.last().unwrap()),
        &json!({"title": "Export", "context": {"source": "github-issue", "ref": "42"}}),
    );
}

#[test]
fn needs_a_task_for_ask_and_done() {
    let result = run(&["done"], |_| json_reply(json!({})));
    assert_eq!(result.code, 2);
    assert!(result.stderr.contains("NATAMUX_TASK_ID"));
    assert!(result.calls.is_empty());
}

// hooks claude-code (hooks.test.ts)

fn printed_settings(env: Env) -> (u8, Value) {
    let result = run_with(&["hooks", "claude-code"], env, b"", |_| {
        panic!("The daemon must not be called")
    });
    assert!(result.calls.is_empty());
    (result.code, serde_json::from_str(&result.stdout).unwrap())
}

#[test]
fn prints_json_without_a_token_and_without_calling_the_daemon() {
    let (code, settings) = printed_settings(Env::default());
    assert_eq!(code, EXIT_OK);
    assert_eq!(
        settings["hooks"]["Stop"][0]["hooks"][0]["url"],
        "http://127.0.0.1:4317/hooks/claude-code"
    );
}

#[test]
fn uses_natamux_url_for_the_address() {
    let (_, settings) = printed_settings(Env::from([("NATAMUX_URL", "http://example.test:9000")]));
    assert_eq!(
        settings["hooks"]["Notification"][0]["hooks"][0]["url"],
        "http://example.test:9000/hooks/claude-code"
    );
}

#[test]
fn does_not_print_the_token_of_the_environment() {
    let (_, settings) = printed_settings(Env::from([("NATAMUX_TOKEN", "super-secret-value")]));
    assert!(!settings.to_string().contains("super-secret-value"));
}
