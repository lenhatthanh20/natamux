//! `describe('graph')`.

use std::fs;

use super::*;

#[test]
fn shows_the_task_branch_next_to_the_branch_of_the_main_checkout() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
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
    let created = fx.ws.create(&task.id, None).unwrap();
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
fn reads_git_now_when_asked_for_a_fresh_graph_and_keeps_that_one() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    let created = fx.ws.create(&task.id, None).unwrap();
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
