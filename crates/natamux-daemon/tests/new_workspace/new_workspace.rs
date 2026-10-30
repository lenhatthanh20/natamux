//! `new-workspace.test.ts`.

use std::fs::{create_dir_all, read_to_string, write};
use std::path::Path;

use natamux_daemon::workspaces::errors::WorkspaceError;
use natamux_daemon::workspaces::new_workspace::{NewWorkspaceInput, new_workspace};
use natamux_proto::tasks::task::TaskState;
use natamux_store::attachments::AttachmentFile;
use serde_json::json;

use crate::{Fx, default_settings};

fn input(title: &str) -> NewWorkspaceInput {
    NewWorkspaceInput {
        title: title.to_string(),
        ..NewWorkspaceInput::default()
    }
}

#[test]
fn makes_a_task_with_a_worktree_and_leaves_the_shell_to_the_user() {
    let fx = Fx::new();
    let created = new_workspace(&fx.new_parts(), input("Fix the login page")).unwrap();
    fx.settle();

    let task = fx.task(&created.id);
    assert_eq!(task.title, "Fix the login page");
    assert_eq!(task.state, TaskState::Draft);
    assert!(task.workspace.is_some());
    assert_eq!(
        task.branch.as_deref(),
        Some("natamux/t-0001-fix-the-login-page")
    );
    assert_eq!(fx.pty_count(), 0);
}

#[test]
fn answers_before_the_worktree_is_made() {
    let fx = Fx::new();
    fx.hold_create();

    let task = new_workspace(&fx.new_parts(), input("Fix the login page")).unwrap();

    assert_eq!(task.id, "T-0001");
    assert_eq!((task.workspace.clone(), task.branch.clone()), (None, None));
    assert_eq!(fx.workspaces.preparing(), [("T-0001".to_string(), "fetch")]);
    fx.release();
    fx.settle();
    assert!(fx.workspaces.preparing().is_empty());
    assert!(fx.task(&task.id).workspace.is_some());
}

#[test]
fn keeps_the_notes_of_a_task_with_no_agent() {
    let fx = Fx::new();
    let notes = "Keep the old URL.\nAsk before a redirect.";

    let task = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            prompt: Some(notes.to_string()),
            ..input("Fix the login page")
        },
    )
    .unwrap();

    assert_eq!(task.description, notes);
    assert_eq!(fx.task(&task.id).description, notes);
    fx.settle();
    assert_eq!(fx.pty_count(), 0);
}

#[test]
fn starts_the_first_stage_with_an_agent_and_the_prompt_becomes_the_notes() {
    let fx = Fx::new();
    fx.hold_create();

    let created = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            agent: Some("claude-code".to_string()),
            prompt: Some("Keep it under 100 ms".to_string()),
            ..input("Speed up search")
        },
    )
    .unwrap();
    assert_eq!(fx.pty_count(), 0);
    fx.release();
    fx.settle();

    let task = fx.task(&created.id);
    assert!(task.workspace.is_some());
    assert_eq!(task.agent.as_deref(), Some("claude-code"));
    assert_eq!(task.description, "Keep it under 100 ms");
    assert_eq!(task.stage, "implement");
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(fx.pty_count(), 1);
}

#[test]
fn starts_no_agent_with_the_default_pipeline_until_a_person_starts_one() {
    let mut fx = Fx::new();
    fx.restart(Some(default_settings()));

    let task = new_workspace(&fx.new_parts(), input("Fix the login page")).unwrap();
    fx.settle();

    let waiting = fx.task(&task.id);
    assert_eq!(
        (waiting.stage.as_str(), waiting.state),
        ("spec", TaskState::Ready)
    );
    assert_eq!(fx.pty_count(), 0);

    fx.start_by_hand(&task.id, "claude-code");
    fx.settle();

    let running = fx.task(&task.id);
    assert_eq!(
        (running.stage.as_str(), running.state),
        ("spec", TaskState::Running)
    );
    assert_eq!(fx.pty_count(), 1);
}

#[test]
fn still_starts_no_agent_after_the_daemon_restarts() {
    let mut fx = Fx::new();
    fx.restart(Some(default_settings()));
    let task = new_workspace(&fx.new_parts(), input("Fix the login page")).unwrap();
    fx.settle();

    fx.restart(Some(default_settings()));
    fx.runner.run.resume_agents();
    let overridden = json!({
        "source": "human",
        "type": "state.overridden",
        "payload": { "to": "ready", "stage": "spec" },
    });
    fx.tasks.apply_event(&task.id, Some(&overridden)).unwrap();
    fx.settle();

    let kept = fx.task(&task.id);
    assert_eq!(kept.without_agent, Some(true));
    assert_eq!(kept.state, TaskState::Ready);
    assert_eq!(fx.pty_count(), 0);
}

#[test]
fn puts_the_attachments_into_the_new_worktree_and_lists_them_in_the_context() {
    let mut fx = Fx::new();
    fx.restart(Some(default_settings()));
    let attachments = vec![AttachmentFile {
        name: "screenshot.png".to_string(),
        bytes: b"image".to_vec(),
    }];

    let task = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            attachments,
            ..input("Fix the login page")
        },
    )
    .unwrap();
    fx.settle();

    let workspace = fx.task(&task.id).workspace.unwrap_or_default();
    let folder = Path::new(&workspace).join(".natamux-task");
    let kept = read_to_string(folder.join("attachments").join("screenshot.png")).unwrap();
    assert_eq!(kept, "image");
    let context = read_to_string(folder.join("context.md")).unwrap();
    assert!(
        context.contains("- `.natamux-task/attachments/screenshot.png` (5 B)"),
        "{context}"
    );
}

#[test]
fn refuses_a_base_branch_that_does_not_exist_and_makes_no_task() {
    let fx = Fx::new();

    let refused = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            base: Some("no-such-branch".to_string()),
            ..input("X")
        },
    );

    assert!(
        matches!(refused, Err(WorkspaceError::Conflict(_))),
        "{refused:?}"
    );
    assert!(fx.tasks.list_tasks(None).unwrap().is_empty());
}

#[test]
fn refuses_a_branch_name_git_does_not_take_and_makes_no_task() {
    let fx = Fx::new();

    let refused = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            branch: Some("bad..name".to_string()),
            ..input("X")
        },
    );

    assert!(
        matches!(refused, Err(WorkspaceError::BadRequest(_))),
        "{refused:?}"
    );
    assert!(fx.tasks.list_tasks(None).unwrap().is_empty());
}

#[test]
fn makes_the_worktree_on_the_named_branch() {
    let fx = Fx::new();

    let created = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            branch: Some("feature-x".to_string()),
            ..input("X")
        },
    )
    .unwrap();
    fx.settle();

    assert_eq!(fx.task(&created.id).branch.as_deref(), Some("feature-x"));
}

// A branch of origin may not be fetched yet, so only the worktree can tell that it is missing.
#[test]
fn makes_the_task_for_a_branch_of_origin_and_fails_its_workspace_when_origin_has_none() {
    let fx = Fx::new();

    let created = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            base: Some("origin/nope".to_string()),
            ..input("X")
        },
    )
    .unwrap();
    fx.settle();

    assert_eq!(fx.task(&created.id).workspace, None);
    let events = fx.events(&created.id);
    let failed = events
        .iter()
        .find(|event| event.kind == "workspace.failed")
        .unwrap();
    assert_eq!(
        failed.payload.get("message").and_then(|text| text.as_str()),
        Some("There is no branch nope on origin")
    );
}

// Node makes `create` reject with a spy. Here git really cannot make the branch: a lock file of
// its ref is there. The text is git's, so the test checks that it names the lock.
#[test]
fn keeps_the_task_and_logs_why_when_the_worktree_cannot_be_made() {
    let fx = Fx::new();
    let heads = fx
        .repo
        .join(".git")
        .join("refs")
        .join("heads")
        .join("natamux");
    create_dir_all(&heads).unwrap();
    write(heads.join("t-0001-x.lock"), "").unwrap();

    let created = new_workspace(
        &fx.new_parts(),
        NewWorkspaceInput {
            agent: Some("claude-code".to_string()),
            ..input("X")
        },
    )
    .unwrap();
    fx.settle();

    let kept = fx.task(&created.id);
    assert_eq!(kept.workspace, None);
    assert_ne!(kept.state, TaskState::Cancelled);
    let events = fx.events(&created.id);
    let failed = events
        .iter()
        .find(|event| event.kind == "workspace.failed")
        .unwrap();
    let message = failed.payload.get("message").and_then(|text| text.as_str());
    assert!(
        message.is_some_and(|text| text.contains(".lock")),
        "{message:?}"
    );
    assert_eq!(fx.pty_count(), 0);
}
