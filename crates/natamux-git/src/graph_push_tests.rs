use super::tests::{
    BRANCH, Fixture, add_worktree, bare_origin, commit_empty, commit_file, diverged_worktree,
    fixture, graph_of, origin_with_main, text,
};
use super::*;
use crate::commit_push::push_branch;
use crate::test_support::run_git;

#[test]
fn has_no_push_status_without_a_remote_named_origin_nor_when_head_is_detached() {
    let f = fixture();
    diverged_worktree(&f);
    assert_eq!(graph_of(&f, Some("main")).push, Some(PushStatus::None));

    bare_origin(&f);
    run_git(&f.worktree, &["checkout", "--detach"]);
    assert_eq!(graph_of(&f, Some("main")).push, None);
}

#[test]
fn counts_the_commits_that_origin_has_on_no_branch_while_the_branch_is_not_pushed() {
    let f = fixture();
    origin_with_main(&f);
    diverged_worktree(&f);

    assert_eq!(
        graph_of(&f, Some("main")).push,
        Some(PushStatus::New { unpushed: 2 })
    );
}

#[test]
fn does_not_count_a_commit_that_is_on_origin_but_not_on_the_local_base_branch() {
    let f = fixture();
    bare_origin(&f);
    commit_empty(&f.dir, "on origin only");
    run_git(&f.dir, &["push", "origin", "main"]);
    run_git(&f.dir, &["reset", "--hard", "HEAD~1"]);
    add_worktree(&f, Some("origin/main"));
    commit_empty(&f.worktree, "one");

    let graph = graph_of(&f, Some("main"));

    assert_eq!(graph.ahead, Some(2));
    assert_eq!(graph.push, Some(PushStatus::New { unpushed: 1 }));
}

#[test]
fn compares_with_origin_branch_once_the_branch_is_pushed() {
    let f = fixture();
    bare_origin(&f);
    diverged_worktree(&f);
    push_branch(&f.worktree, BRANCH).unwrap();
    assert_eq!(
        graph_of(&f, Some("main")).push,
        Some(PushStatus::Tracked {
            unpushed: 0,
            unpulled: 0
        })
    );

    commit_empty(&f.worktree, "three");
    assert_eq!(
        graph_of(&f, Some("main")).push,
        Some(PushStatus::Tracked {
            unpushed: 1,
            unpulled: 0
        })
    );

    // A push from a terminal counts the same as one from the button.
    run_git(&f.worktree, &["push", "origin", BRANCH]);
    assert!(matches!(
        graph_of(&f, Some("main")).push,
        Some(PushStatus::Tracked { unpushed: 0, .. })
    ));
}

#[test]
fn counts_both_sides_when_the_branch_changed_after_a_push() {
    let f = fixture();
    bare_origin(&f);
    diverged_worktree(&f);
    push_branch(&f.worktree, BRANCH).unwrap();
    run_git(
        &f.worktree,
        &["commit", "--amend", "--allow-empty", "-m", "two again"],
    );

    assert_eq!(
        graph_of(&f, Some("main")).push,
        Some(PushStatus::Tracked {
            unpushed: 1,
            unpulled: 1
        })
    );
}

#[test]
fn counts_like_a_new_branch_once_the_branch_is_deleted_on_origin() {
    let f = fixture();
    origin_with_main(&f);
    diverged_worktree(&f);
    push_branch(&f.worktree, BRANCH).unwrap();
    run_git(&f.worktree, &["push", "origin", "--delete", BRANCH]);

    assert_eq!(
        graph_of(&f, Some("main")).push,
        Some(PushStatus::New { unpushed: 2 })
    );
}

fn status_of(f: &Fixture) -> Result<PushStatus, GitError> {
    read_push_status(&f.worktree, BRANCH)
}

#[test]
fn keeps_ahead_and_behind_against_the_base_as_they_were() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    commit_file(&f.worktree, "two");
    commit_file(&f.dir, "main-moved");

    let graph = graph_of(&f, Some("main"));

    assert_eq!((graph.ahead, graph.behind), (Some(2), Some(1)));
    assert_eq!(graph.push, Some(PushStatus::New { unpushed: 2 }));
}

#[test]
fn counts_nothing_for_a_new_branch_that_has_no_commits_of_its_own() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);

    assert_eq!(status_of(&f), Ok(PushStatus::New { unpushed: 0 }));
}

#[test]
fn tells_whether_origin_has_the_branch_from_the_refs_a_push_left() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    assert_eq!(has_remote_branch(&f.worktree, BRANCH), Ok(false));

    run_git(&f.worktree, &["push", "origin", BRANCH]);
    assert_eq!(has_remote_branch(&f.worktree, BRANCH), Ok(true));
}

#[test]
fn counts_the_commits_of_the_local_base_that_origin_does_not_have() {
    let f = fixture();
    origin_with_main(&f);
    commit_file(&f.dir, "local-main");
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");

    assert_eq!(status_of(&f), Ok(PushStatus::New { unpushed: 2 }));
}

#[test]
fn does_not_count_a_commit_that_is_on_another_branch_of_origin() {
    let f = fixture();
    origin_with_main(&f);
    run_git(&f.dir, &["checkout", "-b", "other"]);
    commit_file(&f.dir, "shared");
    run_git(&f.dir, &["push", "origin", "other"]);
    add_worktree(&f, Some("other"));
    commit_file(&f.worktree, "one");

    assert_eq!(status_of(&f), Ok(PushStatus::New { unpushed: 1 }));
}

#[test]
fn compares_with_origin_branch_when_the_branch_has_no_upstream() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    run_git(&f.worktree, &["push", "origin", BRANCH]);
    commit_file(&f.worktree, "two");
    commit_file(&f.worktree, "three");
    commit_file(&f.worktree, "four");

    assert_eq!(
        status_of(&f),
        Ok(PushStatus::Tracked {
            unpushed: 3,
            unpulled: 0
        })
    );
}

#[test]
fn compares_with_origin_branch_when_the_upstream_is_another_branch() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    run_git(&f.worktree, &["push", "origin", BRANCH]);
    run_git(&f.worktree, &["branch", "--set-upstream-to=origin/main"]);
    commit_file(&f.worktree, "two");

    assert_eq!(
        status_of(&f),
        Ok(PushStatus::Tracked {
            unpushed: 1,
            unpulled: 0
        })
    );
}

#[test]
fn looks_only_at_commits_not_at_files_that_are_not_committed() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    push_branch(&f.worktree, BRANCH).unwrap();
    std::fs::write(f.worktree.join("one.txt"), "changed\n").unwrap();
    std::fs::write(f.worktree.join("untracked.txt"), "new\n").unwrap();

    assert_eq!(
        status_of(&f),
        Ok(PushStatus::Tracked {
            unpushed: 0,
            unpulled: 0
        })
    );
}

#[test]
fn counts_commits_that_only_origin_has() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    commit_file(&f.worktree, "two");
    push_branch(&f.worktree, BRANCH).unwrap();
    run_git(&f.worktree, &["reset", "--hard", "HEAD~1"]);

    assert_eq!(
        status_of(&f),
        Ok(PushStatus::Tracked {
            unpushed: 0,
            unpulled: 1
        })
    );
}

#[test]
fn counts_both_sides_after_a_rebase_onto_a_newer_base() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    commit_file(&f.worktree, "two");
    push_branch(&f.worktree, BRANCH).unwrap();
    commit_file(&f.dir, "main-moved");
    run_git(&f.worktree, &["rebase", "main"]);

    // Two rewritten commits, and the new base commit that origin does not have.
    assert_eq!(
        status_of(&f),
        Ok(PushStatus::Tracked {
            unpushed: 3,
            unpulled: 2
        })
    );
}

#[test]
fn counts_like_a_new_branch_after_the_remote_tracking_ref_is_pruned() {
    let f = fixture();
    origin_with_main(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    push_branch(&f.worktree, BRANCH).unwrap();
    let tracking = format!("refs/remotes/origin/{BRANCH}");
    run_git(&f.worktree, &["update-ref", "-d", &tracking]);
    commit_file(&f.worktree, "two");

    assert_eq!(status_of(&f), Ok(PushStatus::New { unpushed: 2 }));
}

#[test]
fn still_compares_with_a_stale_ref_when_the_branch_is_deleted_on_origin_only() {
    let f = fixture();
    let remote = bare_origin(&f);
    run_git(&f.dir, &["push", "origin", "main"]);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    push_branch(&f.worktree, BRANCH).unwrap();
    run_git(&remote, &["branch", "-D", BRANCH]);
    commit_file(&f.worktree, "two");

    assert_eq!(
        status_of(&f),
        Ok(PushStatus::Tracked {
            unpushed: 1,
            unpulled: 0
        })
    );
}

#[test]
fn has_no_remote_when_the_only_remote_has_another_name() {
    let f = fixture();
    let upstream = text(&f.root.path().join("upstream.git"));
    run_git(&f.dir, &["remote", "add", "upstream", &upstream]);
    add_worktree(&f, None);

    assert_eq!(status_of(&f), Ok(PushStatus::None));
}

#[test]
fn counts_every_commit_when_origin_has_no_refs_yet() {
    let f = fixture();
    bare_origin(&f);
    add_worktree(&f, None);
    commit_file(&f.worktree, "one");
    commit_file(&f.worktree, "two");

    assert_eq!(status_of(&f), Ok(PushStatus::New { unpushed: 3 }));
}

#[test]
fn fails_as_before_in_a_repository_with_no_commits() {
    let f = fixture();
    let empty = f.root.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    run_git(&empty, &["init", "-b", "main"]);

    assert!(read_git_graph(&empty, None, MAX_GRAPH_COMMITS).is_err());
}
