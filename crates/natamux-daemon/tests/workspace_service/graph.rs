//! `describe('graph')`.

use std::fs;

use natamux_git::exec::spawn_count;

use super::*;

#[test]
fn shows_the_task_branch_next_to_the_branch_of_the_main_checkout() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    let workspace = PathBuf::from(created.workspace.clone().unwrap());
    run_git(&workspace, &["commit", "--allow-empty", "-m", "work"]);

    let graph = fx.ws.graph(&task.id, false).unwrap();

    assert_eq!(graph.branch, created.branch);
    assert_eq!(graph.base.as_deref(), Some("main"));
    assert_eq!(graph.ahead, Some(1));
    assert_eq!(graph.behind, Some(0));
    let commits: Vec<_> = graph
        .commits
        .iter()
        .map(|c| (c.subject.as_str(), c.on_branch))
        .collect();
    assert_eq!(commits, [("work", true), ("init", false)]);
}

#[test]
fn keeps_the_graph_for_a_moment_and_reads_it_again_after_a_commit_made_here() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    let workspace = PathBuf::from(created.workspace.unwrap());
    let first = fx.ws.graph(&task.id, false).unwrap();
    run_git(&workspace, &["commit", "--allow-empty", "-m", "outside"]);

    // Compared by value: a read that ran again would show the commit made outside.
    assert_eq!(fx.ws.graph(&task.id, false).unwrap(), first);

    fs::write(workspace.join("fix.txt"), "fixed\n").unwrap();
    fx.ws.commit(&task.id, "Fix the bug").unwrap();
    assert_eq!(fx.ws.graph(&task.id, false).unwrap().ahead, Some(2));
}

#[test]
fn runs_no_git_after_a_touch_that_moved_no_ref_and_reads_again_after_one_that_did() {
    let fx = Fx::exclusive();
    let task = fx.with_workspace("Fix the bug");
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    let first = fx.ws.graph(&task.id, false).unwrap();

    // As a hook does after the agent edits a file.
    fs::write(workspace.join("notes.txt"), "draft\n").unwrap();
    fx.ws.touch(&task.id);
    let before = spawn_count();
    assert_eq!(fx.ws.graph(&task.id, false).unwrap(), first);
    assert_eq!(spawn_count() - before, 0);

    // A commit by the agent reaches the daemon only as a touch.
    run_git(&workspace, &["commit", "--allow-empty", "-m", "work"]);
    fx.ws.touch(&task.id);
    let graph = fx.ws.graph(&task.id, false).unwrap();
    assert_eq!(graph.ahead, Some(1));
    assert_eq!(graph.commits[0].subject, "work");
}

#[test]
fn reads_git_now_when_asked_for_a_fresh_graph_and_keeps_that_one() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None, None).unwrap();
    let workspace = PathBuf::from(created.workspace.unwrap());
    fx.ws.graph(&task.id, false).unwrap();
    run_git(&workspace, &["commit", "--allow-empty", "-m", "outside"]);

    let fresh = fx.ws.graph(&task.id, true).unwrap();
    assert_eq!(fresh.ahead, Some(1));
    assert_eq!(fx.ws.graph(&task.id, false).unwrap(), fresh);
}

#[test]
fn refuses_a_task_that_has_no_workspace() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    assert!(is_conflict(&fx.ws.graph(&task.id, false)));
}
