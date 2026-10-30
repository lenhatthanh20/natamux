//! `describe('commit, push and pull request')`. The fixture's gh answers with `PR_URL` and records
//! its arguments (`gh_args`).

use std::fs;

use super::*;

fn task_with_work(fx: &Fx) -> Task {
    let task = fx.task_with(json!({ "title": "Fix the bug", "description": "The details." }));
    let created = fx.ws.create(&task.id, None, None).unwrap();
    fs::write(
        PathBuf::from(created.workspace.clone().unwrap()).join("fix.txt"),
        "fixed\n",
    )
    .unwrap();
    created
}

fn last_event(fx: &Fx, task_id: &str) -> natamux_proto::events::task_event::TaskEvent {
    fx.events(task_id).pop().unwrap().event
}

fn has_branch(remote: &Path, branch: &str) -> bool {
    !run_git(remote, &["branch", "--list", branch])
        .trim()
        .is_empty()
}

#[test]
fn commits_the_changes_of_the_workspace_and_logs_the_commit() {
    let fx = Fx::new();
    let task = task_with_work(&fx);

    // Kept from before the commit, which must not show up afterwards.
    assert_eq!(fx.ws.changes(&task.id, false).unwrap().len(), 1);
    let (_, hash) = fx.ws.commit(&task.id, "Fix the bug").unwrap();

    assert!(fx.ws.changes(&task.id, false).unwrap().is_empty());
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    assert_eq!(run_git(&workspace, &["rev-parse", "HEAD"]).trim(), hash);
    let event = last_event(&fx, &task.id);
    assert_eq!(event.kind, "git.committed");
    assert_eq!(event.payload["hash"], json!(hash));
    assert_eq!(event.payload["subject"], json!("Fix the bug"));
    let again = fx.ws.commit(&task.id, "Again");
    assert!(matches!(again, Err(WorkspaceError::NothingToCommit)));
}

#[test]
fn pushes_the_task_branch_to_origin() {
    let fx = Fx::new();
    let remote = bare_origin(fx.root.path());
    let task = task_with_work(&fx);
    fx.ws.commit(&task.id, "Fix the bug").unwrap();

    let pushed = fx.ws.push(&task.id).unwrap();

    assert!(has_branch(&remote, task.branch.as_deref().unwrap()));
    let event = last_event(&fx, &task.id);
    assert_eq!(event.kind, "git.pushed");
    assert_eq!(event.payload["branch"], json!(task.branch));
    assert_eq!(pushed.pr, None);
}

#[test]
fn pushes_the_branch_the_agent_switched_to() {
    let fx = Fx::new();
    let remote = bare_origin(fx.root.path());
    let task = task_with_work(&fx);
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    run_git(&workspace, &["switch", "-c", "feature-x"]);
    fx.ws.commit(&task.id, "Fix the bug").unwrap();

    let pushed = fx.ws.push(&task.id).unwrap();

    assert_eq!(pushed.branch.as_deref(), Some("feature-x"));
    assert!(has_branch(&remote, "feature-x"));
    assert!(!has_branch(&remote, task.branch.as_deref().unwrap()));
    assert_eq!(last_event(&fx, &task.id).payload["branch"], json!("feature-x"));
}

#[test]
fn pushes_then_opens_a_pull_request_and_keeps_its_address() {
    let fx = Fx::new();
    bare_origin(fx.root.path());
    let task = task_with_work(&fx);
    fx.ws.commit(&task.id, "Fix the bug").unwrap();

    let updated = fx.ws.open_pull_request(&task.id).unwrap();

    assert_eq!(updated.pr.as_deref(), Some(PR_URL));
    let head = task.branch.clone().unwrap();
    let expected = strings(&[
        "pr",
        "create",
        "--head",
        &head,
        "--title",
        "Fix the bug",
        "--body",
        "The details.",
    ]);
    assert_eq!(fx.gh_args(), [expected]);
    let kinds = fx.event_kinds(&task.id);
    assert_eq!(kinds[kinds.len() - 2..], ["git.pushed", "pr.opened"]);
    assert!(is_conflict(&fx.ws.open_pull_request(&task.id)));
}

#[test]
fn pushes_the_parent_branch_and_opens_the_pull_request_of_a_subtask_into_it() {
    let fx = Fx::new();
    let remote = bare_origin(fx.root.path());
    let parent = fx.with_workspace("Parent");
    let child = fx.task_with(json!({ "title": "Part A", "parentId": parent.id }));
    let created = fx.ws.create(&child.id, None, None).unwrap();
    fs::write(
        PathBuf::from(created.workspace.clone().unwrap()).join("part.txt"),
        "part\n",
    )
    .unwrap();
    fx.ws.commit(&child.id, "Part A").unwrap();

    fx.ws.open_pull_request(&child.id).unwrap();

    let (head, base) = (
        created.branch.clone().unwrap(),
        parent.branch.clone().unwrap(),
    );
    assert!(has_branch(&remote, &base));
    let body = format!("Task {} in Natamux.", child.id);
    let expected = strings(&[
        "pr", "create", "--head", &head, "--base", &base, "--title", "Part A", "--body", &body,
    ]);
    assert_eq!(fx.gh_args(), [expected]);
}

#[test]
fn opens_a_later_subtask_pull_request_when_origin_has_the_parent_branch_ahead() {
    let fx = Fx::new();
    let remote = bare_origin(fx.root.path());
    let parent = fx.with_workspace("Parent");
    let base = parent.branch.clone().unwrap();
    run_git(
        &PathBuf::from(parent.workspace.clone().unwrap()),
        &["push", "-u", "origin", &base],
    );
    // A subtask merged into the parent branch on GitHub: the local copy is behind.
    let other = fx.root.path().join("other");
    run_git(
        fx.root.path(),
        &[
            "clone",
            "--branch",
            &base,
            path_text(&remote),
            path_text(&other),
        ],
    );
    run_git(&other, &["commit", "--allow-empty", "-m", "merged subtask"]);
    run_git(&other, &["push", "origin", &base]);
    let merged = run_git(&other, &["rev-parse", "HEAD"]);
    let child = fx.task_with(json!({ "title": "Part B", "parentId": parent.id }));
    let created = fx.ws.create(&child.id, None, None).unwrap();
    fs::write(
        PathBuf::from(created.workspace.clone().unwrap()).join("part.txt"),
        "part\n",
    )
    .unwrap();
    fx.ws.commit(&child.id, "Part B").unwrap();

    let updated = fx.ws.open_pull_request(&child.id).unwrap();

    assert_eq!(updated.pr.as_deref(), Some(PR_URL));
    assert_eq!(fx.gh_args()[0][4..6], ["--base", base.as_str()]);
    assert_eq!(
        run_git(&remote, &["rev-parse", &base]).trim(),
        merged.trim()
    );
}

#[test]
fn does_not_ask_gh_when_the_push_fails() {
    let fx = Fx::new();
    let task = task_with_work(&fx);

    assert!(is_git(&fx.ws.open_pull_request(&task.id)));

    assert!(fx.gh_args().is_empty());
    assert_eq!(fx.tasks.get_task(&task.id).unwrap().pr, None);
}

#[test]
fn refuses_a_task_without_a_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");

    assert!(is_conflict(&fx.ws.commit(&task.id, "x")));
    assert!(is_conflict(&fx.ws.push(&task.id)));
}
