//! Each route screen 1 calls: its method, path and body, and how its answer is read.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_client::ClientError;
use natamux_proto::agents::start::StartAgentInput;
use natamux_proto::events::task_event::TaskEventInput;
use natamux_proto::tasks::task::{NewTaskInput, TemplateTaskInput};
use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::{NewWorkspaceInput, PreparingStep, PushStatus};
use serde_json::{Value, json};
use support::{exchange, ok};

const TASK: &str =
    r#"{"id":"T-0009","title":"Fix it","state":"draft","updatedAt":"2026-10-22T00:00:00.000Z"}"#;

#[test]
fn reads_the_task_list() {
    let (tasks, sent) = ok(
        "GET /tasks",
        &format!(r#"{{"tasks":[{TASK}]}}"#),
        |client| client.list_tasks(),
    );
    assert_eq!(tasks[0].id, "T-0009");
    assert_eq!(sent, Value::Null);
}

#[test]
fn reads_projects_with_the_worktrees_folder() {
    let body = r#"{"repos":[{"id":"natamux","name":"natamux","path":"/r/natamux","worktreesDir":"/wt"}],"worktreesDir":"/data/worktrees"}"#;
    let (list, _) = ok("GET /repos", body, |client| client.list_repos());
    assert_eq!(list.repos[0].worktrees_dir.as_deref(), Some("/wt"));
    assert_eq!(list.worktrees_dir.as_deref(), Some("/data/worktrees"));
}

#[test]
fn reads_templates_and_jobs() {
    let body = r#"{"templates":[{"id":"bug","name":"Bug","title":"Bug: ","labels":["bug"]}]}"#;
    let (templates, _) = ok("GET /templates", body, |client| client.list_templates());
    assert_eq!(templates[0].priority, "normal");
    let body = r#"{"jobs":[{"id":"j1","kind":"push","label":"Pushing","taskId":"T-0009","startedAt":"2026-10-22T01:00:00.000Z"}]}"#;
    let (jobs, _) = ok("GET /jobs", body, |client| client.list_jobs());
    assert_eq!(jobs[0].task_id.as_deref(), Some("T-0009"));
}

#[test]
fn reads_activity_diff_stats_and_usage() {
    let body =
        r#"{"activity":{"T-0009":{"1":{"text":"Edit a.ts","at":"2026-10-22T01:00:00.000Z"}}}}"#;
    let (activity, _) = ok("GET /activity", body, |client| client.activity());
    assert_eq!(activity["T-0009"]["1"].text, "Edit a.ts");
    let body = r#"{"stats":{"T-0009":{"files":2,"added":10,"removed":3}}}"#;
    let (stats, _) = ok("GET /diffstats", body, |client| client.diff_stats());
    assert_eq!(stats["T-0009"].added, 10);
    let body =
        r#"{"usage":{"T-0009":{"tokens":1200,"costUsd":0.5,"unpricedModels":[],"stages":[]}}}"#;
    let (usage, _) = ok("GET /usage", body, |client| client.usage());
    assert_eq!(usage["T-0009"].tokens, 1200);
}

#[test]
fn reads_the_workspaces_being_made() {
    let body = r#"{"preparing":{"T-0009":"fetch"}}"#;
    let (preparing, _) = ok("GET /workspaces/preparing", body, |client| {
        client.list_preparing()
    });
    assert_eq!(preparing["T-0009"], PreparingStep::Fetch);
}

#[test]
fn reads_branches_of_a_project_or_of_the_default_one() {
    let body = r#"{"branches":["dev","main"],"current":"main","defaultBranch":"main"}"#;
    let (branches, _) = ok("GET /git/branches?repo=natamux", body, |client| {
        client.list_branches(Some("natamux"))
    });
    assert_eq!(branches.branches, ["dev", "main"]);
    assert_eq!(branches.default_branch.as_deref(), Some("main"));
    let (_, _) = ok("GET /git/branches", body, |client| {
        client.list_branches(None)
    });
}

#[test]
fn reads_the_branches_of_origin() {
    let body = r#"{"branches":["origin/dev","origin/main"]}"#;
    let (remote, _) = ok("GET /git/remote-branches?repo=natamux", body, |client| {
        client.list_remote_branches("natamux")
    });
    assert_eq!(remote.branches, ["origin/dev", "origin/main"]);
}

#[test]
fn reads_project_settings_and_skills() {
    let body = r#"{"settings":{"context":{"sources":[{"type":"manual"}]}},"found":true,"version":"v1","path":"/r/.natamux.json"}"#;
    let (loaded, _) = ok("GET /repos/natamux/settings", body, |client| {
        client.load_settings("natamux")
    });
    assert_eq!(loaded.settings.context.sources[0].kind, "manual");
    assert!(loaded.found);
    let body = r#"{"skills":[{"name":"docs","description":"Docs","origin":"project","agents":["claude-code"],"path":"/s"}],"notices":[]}"#;
    let (skills, _) = ok("GET /repos/natamux/skills", body, |client| {
        client.list_skills("natamux")
    });
    assert_eq!(skills.skills[0].name, "docs");
}

#[test]
fn reads_the_terminals_events_changes_and_graph_of_a_task() {
    let body = r#"{"terminals":["1","2"],"agents":["1"],"lanes":{"1":{"stage":"implement","state":"running","agent":"claude-code"}},"programs":{"1":"claude","2":null},"restoring":{}}"#;
    let (terminals, _) = ok("GET /tasks/T-0009/terminals", body, |client| {
        client.list_terminals("T-0009")
    });
    assert_eq!(terminals.terminals, ["1", "2"]);
    assert!(terminals.lanes["1"].is_agent());

    let body = r#"{"events":[{"seq":1,"accepted":true,"event":{"taskId":"T-0009","source":"agent","type":"agent.asked","at":"2026-10-22T01:00:00.000Z","payload":{"message":"Which one?"}}}]}"#;
    let (events, _) = ok("GET /tasks/T-0009/events", body, |client| {
        client.list_events("T-0009")
    });
    assert_eq!(events[0].event.question().as_deref(), Some("Which one?"));

    let body = r#"{"changes":[{"path":"a.ts","status":"M","added":1,"removed":0}]}"#;
    let (changes, _) = ok("GET /tasks/T-0009/changes", body, |client| {
        client.list_changes("T-0009", false)
    });
    assert_eq!(changes[0].status, ChangeStatus::Modified);

    let body = r#"{"branch":"natamux/t-0009","head":"abc","base":"main","ahead":2,"behind":0,"push":{"remote":"new","unpushed":2},"commits":[],"truncated":false}"#;
    let (graph, _) = ok("GET /tasks/T-0009/graph", body, |client| {
        client.graph("T-0009", false)
    });
    assert_eq!(graph.push, Some(PushStatus::New { unpushed: 2 }));
}

#[test]
fn creates_a_task_and_a_subtask() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (task, sent) = ok("POST /tasks", &answer, |client| {
        client.create_task(NewTaskInput {
            title: "Fix it".into(),
            parent_id: Some("T-0001".into()),
            repo: Some("natamux".into()),
        })
    });
    assert_eq!(task.id, "T-0009");
    assert_eq!(
        sent,
        json!({"title": "Fix it", "parentId": "T-0001", "repo": "natamux"})
    );
}

#[test]
fn creates_a_task_from_a_template() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (_, sent) = ok("POST /templates/bug/tasks", &answer, |client| {
        client.create_from_template(
            "bug",
            TemplateTaskInput {
                title: "Bug: crash".into(),
                repo: None,
            },
        )
    });
    assert_eq!(sent, json!({"title": "Bug: crash"}));
}

#[test]
fn starts_a_workspace() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (task, sent) = ok("POST /workspaces", &answer, |client| {
        client.new_workspace(NewWorkspaceInput {
            title: "Fix it".into(),
            repo: Some("natamux".into()),
            base: Some("main".into()),
            agent: Some("claude-code".into()),
            ..NewWorkspaceInput::default()
        })
    });
    assert_eq!(task.id, "T-0009");
    assert_eq!(
        sent,
        json!({"title": "Fix it", "repo": "natamux", "base": "main", "agent": "claude-code"})
    );
}

#[test]
fn sends_an_event_as_a_person() {
    let answer = format!(r#"{{"task":{TASK},"accepted":true,"moved":true}}"#);
    let (answer, sent) = ok("POST /tasks/T-0009/events", &answer, |client| {
        client.send_event("T-0009", TaskEventInput::human("task.readied"))
    });
    assert!(answer.accepted);
    assert_eq!(sent, json!({"source": "human", "type": "task.readied"}));
}

#[test]
fn renames_a_task() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (_, sent) = ok("PATCH /tasks/T-0009", &answer, |client| {
        client.rename_task("T-0009", "Fix it now")
    });
    assert_eq!(sent, json!({"title": "Fix it now"}));
}

#[test]
fn deletes_a_task() {
    let answer = format!(r#"{{"id":"T-0009","tasks":[{TASK}]}}"#);
    let (answer, sent) = ok("DELETE /tasks/T-0009", &answer, |client| {
        client.delete_task("T-0009")
    });
    assert_eq!(answer.id, "T-0009");
    assert_eq!(answer.tasks.len(), 1);
    assert_eq!(sent, Value::Null);
}

#[test]
fn removes_a_workspace_with_or_without_its_branch() {
    let answer = format!(r#"{{"task":{TASK},"branchDeleted":true}}"#);
    let (removed, _) = ok(
        "DELETE /tasks/T-0009/workspace?deleteBranch=true",
        &answer,
        |client| client.remove_workspace("T-0009", true),
    );
    assert!(removed.branch_deleted);
    let answer = format!(r#"{{"task":{TASK},"branchDeleted":false}}"#);
    let (removed, _) = ok(
        "DELETE /tasks/T-0009/workspace?deleteBranch=false",
        &answer,
        |client| client.remove_workspace("T-0009", false),
    );
    assert!(!removed.branch_deleted);
}

#[test]
fn removes_a_project_that_answers_with_no_body() {
    let (answer, sent) = exchange("DELETE /repos/natamux", "204 No Content", "", |client| {
        client.remove_repo("natamux")
    });
    assert_eq!(answer, Ok(()));
    assert_eq!(sent, Value::Null);
}

#[test]
fn starts_an_agent() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (_, sent) = ok("POST /tasks/T-0009/agent", &answer, |client| {
        client.start_agent(
            "T-0009",
            StartAgentInput {
                agent: "codex".into(),
                prompt: Some("Go".into()),
                base: None,
            },
        )
    });
    assert_eq!(sent, json!({"agent": "codex", "prompt": "Go"}));
}

#[test]
fn ids_are_encoded_in_the_path() {
    let (_, _) = ok("GET /tasks/T%2F1/events", r#"{"events":[]}"#, |client| {
        client.list_events("T/1")
    });
}

#[test]
fn a_refused_write_gives_the_daemons_message() {
    let (answer, _) = exchange(
        "DELETE /tasks/T-0009",
        "409 Conflict",
        r#"{"error":"conflict","message":"T-0009 has uncommitted changes"}"#,
        |client| client.delete_task("T-0009"),
    );
    assert_eq!(
        answer.unwrap_err(),
        ClientError::Status {
            status: 409,
            message: "T-0009 has uncommitted changes".into()
        }
    );
}

#[test]
fn creates_a_workspace_with_no_body_or_with_a_base() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (task, sent) = exchange(
        "POST /tasks/T-0009/workspace",
        "201 Created",
        &answer,
        |c| c.create_workspace("T-0009", None),
    );
    assert_eq!(task.unwrap().id, "T-0009");
    assert_eq!(sent, Value::Null);
    let (_, sent) = exchange(
        "POST /tasks/T-0009/workspace",
        "201 Created",
        &answer,
        |c| c.create_workspace("T-0009", Some("dev")),
    );
    assert_eq!(sent, json!({"base": "dev"}));
}

#[test]
fn pushes_opens_a_pull_request_and_reopens_an_agent_with_no_body() {
    let answer = format!(r#"{{"task":{TASK}}}"#);
    let (task, sent) = ok("POST /tasks/T-0009/push", &answer, |c| c.push("T-0009"));
    assert_eq!((task.id.as_str(), sent), ("T-0009", Value::Null));
    let (task, sent) = ok("POST /tasks/T-0009/pull-request", &answer, |c| {
        c.open_pull_request("T-0009")
    });
    assert_eq!((task.id.as_str(), sent), ("T-0009", Value::Null));
    let (task, sent) = ok("POST /tasks/T-0009/agent/reopen", &answer, |c| {
        c.reopen_agent("T-0009")
    });
    assert_eq!((task.id.as_str(), sent), ("T-0009", Value::Null));
}

#[test]
fn a_refused_push_gives_the_daemons_message() {
    let (answer, _) = exchange(
        "POST /tasks/T-0009/push",
        "409 Conflict",
        r#"{"error":"conflict","message":"no remote"}"#,
        |c| c.push("T-0009"),
    );
    assert_eq!(
        answer.unwrap_err(),
        ClientError::Status {
            status: 409,
            message: "no remote".into()
        }
    );
}

#[test]
fn reads_the_ended_shells_of_a_task() {
    let body = r#"{"shells":[{"name":"1","agent":"claude-code","stage":"spec","session":null,"endedAt":"2026-10-22T00:00:00.000Z","text":"done"}]}"#;
    let (shells, sent) = ok("GET /tasks/T-0009/ended-shells", body, |c| {
        c.ended_shells("T-0009")
    });
    assert_eq!(shells[0].text, "done");
    assert_eq!(shells[0].session, None);
    assert_eq!(sent, Value::Null);
}
