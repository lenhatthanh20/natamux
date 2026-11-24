//! `delete_branches_of`: the CI pins and the CI branches of origin a task leaves, and a project
//! that is gone.

use super::*;

/// A task whose worktree is gone and whose branch stays, as a delete sees it.
fn removed_task(fx: &Fx) -> Task {
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None, None).unwrap();
    fx.ws.remove(&task.id, false).unwrap().task
}

fn ci_round(fx: &Fx, task_id: &str, ci_branch: &str) {
    let payload = json!({ "stage": "implement", "branch": ci_branch });
    fx.tasks
        .record_event(task_id, "stage.ci-approved", payload, "system")
        .unwrap();
}

fn push_ci_branch(fx: &Fx, ci_branch: &str) {
    let target = format!("HEAD:refs/heads/{ci_branch}");
    run_git(&fx.repo, &["push", "-q", "origin", &target]);
}

fn heads(origin: &Path) -> String {
    run_git(origin, &["for-each-ref", "--format=%(refname)"])
}

#[test]
fn deletes_the_branch_of_an_unfinished_ci_task_and_only_logs_a_failed_ci_branch_delete() {
    let fx = Fx::new();
    let removed = removed_task(&fx);
    assert!(fx.branch_exists(BRANCH));
    ci_round(&fx, &removed.id, &format!("natamux-ci/{BRANCH}"));

    fx.ws.delete_branches_of(&removed).unwrap();

    assert!(!fx.branch_exists(BRANCH));
    assert!(
        fx.log_lines()
            .iter()
            .any(|line| line.contains("Could not delete the CI branch"))
    );
}

#[test]
fn deletes_the_ci_branch_of_every_round_and_of_the_branch_now() {
    let fx = Fx::new();
    let origin = bare_origin(fx.root.path());
    let removed = removed_task(&fx);
    let own = format!("natamux-ci/{BRANCH}");
    for ci_branch in ["natamux-ci/a", "natamux-ci/b", own.as_str()] {
        push_ci_branch(&fx, ci_branch);
    }
    ci_round(&fx, &removed.id, "natamux-ci/a");
    ci_round(&fx, &removed.id, "natamux-ci/b");
    // Turned off after the rounds: the branches still go.
    let removed = Task {
        ci_stages: Some(vec![]),
        ..removed
    };

    fx.ws.delete_branches_of(&removed).unwrap();

    assert_eq!(heads(&origin), "");
}

#[test]
fn a_task_that_never_ran_a_ci_round_makes_no_remote_call() {
    let fx = Fx::new();
    let origin = bare_origin(fx.root.path());
    let removed = removed_task(&fx);
    let ci_branch = format!("natamux-ci/{BRANCH}");
    push_ci_branch(&fx, &ci_branch);
    let removed = Task {
        ci_stages: Some(strings(&["implement"])),
        ..removed
    };

    fx.ws.delete_branches_of(&removed).unwrap();

    assert!(heads(&origin).contains(&ci_branch));
}

#[test]
fn goes_on_when_the_project_of_a_branch_only_task_is_gone() {
    let fx = Fx::new();
    let removed = removed_task(&fx);
    let orphan = Task {
        repo: Some("removed-project".into()),
        ..removed
    };

    fx.ws.delete_branches_of(&orphan).unwrap();

    assert!(fx.branch_exists(BRANCH));
    assert!(
        fx.log_lines()
            .iter()
            .any(|line| line.contains("Could not delete the branch"))
    );
}

#[test]
fn writes_no_line_for_a_task_with_no_branch_whose_project_is_gone() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let orphan = Task {
        repo: Some("removed-project".into()),
        branch: None,
        ..task
    };

    fx.ws.delete_branches_of(&orphan).unwrap();

    let lines = fx.log_lines();
    assert!(
        !lines.iter().any(|line| line.contains("Could not delete")),
        "{lines:?}"
    );
}

#[test]
fn removes_the_ci_pins_of_a_task_that_has_no_finished_ci_stages() {
    let fx = Fx::new();
    let removed = removed_task(&fx);
    let pin = format!("refs/natamux/ci/{}/implement", removed.id);
    run_git(&fx.repo, &["update-ref", &pin, "HEAD"]);

    fx.ws.delete_branches_of(&removed).unwrap();

    let left = run_git(&fx.repo, &["for-each-ref", "refs/natamux/ci/"]);
    assert!(left.trim().is_empty(), "pins left: {left}");
}
