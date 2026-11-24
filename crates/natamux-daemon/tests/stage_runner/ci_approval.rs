//! F4: a CI round sends only what was approved before it started. The approval is recorded first,
//! a restart pushes the same commit, and Strict `ciPush` modes change what goes or stop it.

use std::path::{Path, PathBuf};

use natamux_core::security::{
    CI_FORBID_REASON, CiMode, CiPush, SecurityMode, StrictSettings, force,
};
use natamux_daemon::stages::ci_approval::NO_APPROVAL;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::StageAction;
use natamux_stages::ci::CiResult;
use serde_json::{Value, json};

use crate::{Fixture, run_git, term_input, wait_for};

/// A project with a bare origin, whose Implement stage offers CI.
fn fixture() -> Fixture {
    let f = Fixture::new();
    f.set_stages(json!([{ "id": "implement", "kind": "agent", "gate": "manual", "ci": true }]));
    let origin = origin_of(&f);
    run_git(
        &f.dir.0,
        &["init", "-q", "--bare", origin.to_str().unwrap()],
    );
    run_git(
        &f.repo,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    f
}

fn origin_of(f: &Fixture) -> PathBuf {
    f.dir.0.join("origin.git")
}

fn strict(ci_push: CiPush) -> SecurityMode {
    SecurityMode::Strict(StrictSettings {
        ci_push,
        ..StrictSettings::default()
    })
}

/// No settle: a CI round waits in the fake check until the test finishes it.
fn report(f: &Fixture, id: &str) {
    f.runner
        .run
        .act(id, StageAction::Report, &term_input("1"))
        .unwrap();
}

/// A task at Implement with CI on, a tracked change and an untracked fake secret in its worktree.
fn dirty_task(f: &Fixture) -> Task {
    let task = f.implementing("implement");
    f.set_ci(&task.id, "implement", true);
    let worktree = worktree(&task);
    std::fs::write(worktree.join("tracked.rs"), "fn a() {}\n").unwrap();
    run_git(&worktree, &["add", "tracked.rs"]);
    run_git(&worktree, &["commit", "-q", "-m", "tracked"]);
    std::fs::write(worktree.join("tracked.rs"), "fn b() {}\n").unwrap();
    std::fs::write(worktree.join("secret.txt"), "FAKE-SECRET\n").unwrap();
    task
}

fn worktree(task: &Task) -> PathBuf {
    PathBuf::from(task.workspace.clone().unwrap())
}

fn seq_of(f: &Fixture, id: &str, kind: &str) -> Option<u64> {
    let events = f.tasks.list_events(id).unwrap();
    events
        .iter()
        .rev()
        .find(|stored| stored.event.kind == kind)
        .map(|stored| stored.seq)
}

fn count_of(f: &Fixture, id: &str, kind: &str) -> usize {
    f.task_events(id)
        .iter()
        .filter(|event| event.kind == kind)
        .count()
}

fn payload(f: &Fixture, id: &str, kind: &str) -> serde_json::Map<String, Value> {
    f.last_of(id, kind).unwrap().payload
}

fn calls(f: &Fixture) -> usize {
    f.ci.calls.lock().unwrap().len()
}

fn names_in(worktree: &Path, commit: &str) -> String {
    run_git(worktree, &["ls-tree", "-r", "--name-only", commit])
}

fn passed() -> CiResult {
    CiResult {
        passed: true,
        ..CiResult::default()
    }
}

#[test]
fn records_the_approval_before_the_round_and_pushes_exactly_that_commit() {
    let f = fixture();
    let task = dirty_task(&f);
    report(&f, &task.id);
    wait_for(|| calls(&f) == 1);

    let approved = payload(&f, &task.id, "stage.ci-approved");
    let approval_seq = seq_of(&f, &task.id, "stage.ci-approved").unwrap();
    assert!(approval_seq < seq_of(&f, &task.id, "stage.ci-started").unwrap());
    assert_eq!(approved["mode"], "snapshot");
    assert_eq!(approved["approver"], "auto");
    assert_eq!(approved["files"], json!(["tracked.rs"]));
    assert!(approved["remote"].as_str().unwrap().ends_with("origin.git"));
    let started = payload(&f, &task.id, "stage.ci-started");
    assert_eq!(started["approval"], approval_seq);
    assert!(
        started["note"].as_str().unwrap().contains("secret.txt"),
        "{started:?}"
    );
    let target = f.ci.targets.lock().unwrap()[0].clone();
    assert_eq!(target.commit, approved["commit"].as_str().unwrap());
    assert_eq!(target.tree, approved["tree"].as_str().unwrap());
    assert_eq!(target.mode, CiMode::Snapshot);
    let names = names_in(&worktree(&task), &target.commit);
    assert!(names.contains("tracked.rs") && !names.contains("secret.txt"));
    let pin = format!("refs/natamux/ci/{}/implement", task.id);
    assert_eq!(run_git(&f.repo, &["rev-parse", &pin]).trim(), target.commit);

    f.ci.finish(passed());
    f.settle();
    assert_eq!(f.task(&task.id).state, TaskState::Review);
}

#[test]
fn a_restart_with_a_dirty_worktree_pushes_the_approved_commit_again() {
    let mut f = fixture();
    let task = dirty_task(&f);
    report(&f, &task.id);
    wait_for(|| calls(&f) == 1);
    std::fs::write(worktree(&task).join("tracked.rs"), "fn later() {}\n").unwrap();

    f.runner.stop();
    f.runner = f.make_runner(None);
    f.runner.start();
    wait_for(|| calls(&f) == 2);

    let targets = f.ci.targets.lock().unwrap().clone();
    assert_eq!(targets[0], targets[1]);
    assert_eq!(count_of(&f, &task.id, "stage.ci-approved"), 1);
    f.ci.finish(passed());
    f.settle();
}

#[test]
fn a_round_without_its_approval_fails_for_good_and_pushes_nothing() {
    let mut f = fixture();
    let task = f.implementing("implement");
    f.set_ci(&task.id, "implement", true);
    f.runner.stop();
    let started = json!({ "stage": "implement", "branch": "natamux-ci/x" });
    f.tasks
        .record_event(&task.id, "stage.ci-started", started, "system")
        .unwrap();

    f.runner = f.make_runner(None);
    f.runner.start();
    f.settle();

    assert_eq!(calls(&f), 0);
    assert_eq!(
        Value::Object(payload(&f, &task.id, "stage.check-failed")),
        json!({ "stage": "implement", "reasons": [NO_APPROVAL], "final": true })
    );
    assert!(f.last_of(&task.id, "stage.reopened").is_none());
}

#[test]
fn approved_commit_sends_head_and_says_what_is_not_sent() {
    let f = fixture();
    let _strict = force(strict(CiPush::ApprovedCommit));
    let task = dirty_task(&f);
    report(&f, &task.id);
    wait_for(|| calls(&f) == 1);

    let head = run_git(&worktree(&task), &["rev-parse", "HEAD"]);
    let target = f.ci.targets.lock().unwrap()[0].clone();
    assert_eq!(target.commit, head.trim());
    assert_eq!(target.mode, CiMode::ApprovedCommit);
    let note = payload(&f, &task.id, "stage.ci-started")["note"].clone();
    let short: String = head.trim().chars().take(7).collect();
    assert!(
        note.as_str()
            .unwrap()
            .starts_with(&format!("CI checks the last commit ({short})."))
    );
    f.ci.finish(passed());
    f.settle();
}

#[test]
fn approved_snapshot_waits_for_the_exact_tree_and_then_sends_it() {
    let f = fixture();
    let _strict = force(strict(CiPush::ApprovedSnapshot));
    let task = dirty_task(&f);
    report(&f, &task.id);
    f.settle();

    let needed = payload(&f, &task.id, "stage.ci-approval-needed");
    assert_eq!(needed["files"], json!(["tracked.rs"]));
    assert_eq!(calls(&f), 0);
    assert!(f.last_of(&task.id, "stage.ci-started").is_none());
    let tree = needed["tree"].as_str().unwrap().to_string();
    std::fs::write(worktree(&task).join("tracked.rs"), "fn later() {}\n").unwrap();

    let wrong = f.runner.run.approve_ci(&task.id, "implement", "0000");
    assert!(wrong.unwrap_err().to_string().contains("not 0000"));
    assert!(f.last_of(&task.id, "stage.ci-approved").is_none());
    f.runner
        .run
        .approve_ci(&task.id, "implement", &tree)
        .unwrap();
    f.bus.deliver_all();
    wait_for(|| calls(&f) == 1);

    let approved = payload(&f, &task.id, "stage.ci-approved");
    assert_eq!(approved["approver"], "human");
    let target = f.ci.targets.lock().unwrap()[0].clone();
    assert_eq!(target.tree, tree);
    assert_eq!(target.commit, needed["commit"].as_str().unwrap());
    let content = run_git(
        &worktree(&task),
        &["show", &format!("{}:tracked.rs", target.commit)],
    );
    assert_eq!(content, "fn b() {}\n");
    assert!(!names_in(&worktree(&task), &target.commit).contains("secret.txt"));
    f.ci.finish(passed());
    f.settle();
}

#[test]
fn forbid_fails_for_good_with_no_snapshot_and_no_push() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_ci(&task.id, "implement", true);
    let _forbid = force(strict(CiPush::Forbid));

    report(&f, &task.id);
    f.settle();

    assert_eq!(calls(&f), 0);
    assert!(f.last_of(&task.id, "stage.ci-approved").is_none());
    assert_eq!(
        payload(&f, &task.id, "stage.ci-blocked")["reason"],
        CI_FORBID_REASON
    );
    assert_eq!(
        Value::Object(payload(&f, &task.id, "stage.check-failed")),
        json!({ "stage": "implement", "reasons": [CI_FORBID_REASON], "final": true })
    );
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    let pins = run_git(&f.repo, &["for-each-ref", "refs/natamux/ci/"]);
    assert_eq!(pins, "");
}

#[test]
fn forbid_stops_an_old_round_after_a_restart_with_the_reason() {
    let mut f = fixture();
    let task = dirty_task(&f);
    report(&f, &task.id);
    wait_for(|| calls(&f) == 1);
    f.runner.stop();
    let _forbid = force(strict(CiPush::Forbid));

    f.runner = f.make_runner(None);
    f.runner.start();
    f.settle();

    assert_eq!(calls(&f), 1);
    assert_eq!(
        payload(&f, &task.id, "stage.check-failed")["reasons"],
        json!([CI_FORBID_REASON])
    );
}

#[test]
fn forbid_refuses_turning_ci_on_but_not_off() {
    let f = fixture();
    let task = f.implementing("implement");
    f.set_ci(&task.id, "implement", true);
    let _forbid = force(strict(CiPush::Forbid));

    let off = f.tasks.set_ci(&task.id, "implement", false).unwrap();
    assert_eq!(off.ci_stages, Some(vec![]));
    let on = f.tasks.set_ci(&task.id, "implement", true);
    assert!(on.unwrap_err().to_string().contains("forbids CI pushes"));
    assert_eq!(f.task(&task.id).ci_stages, Some(vec![]));
}

/// The CI branch of a finished task goes, except under `forbid`: then no remote call is made.
#[test]
fn the_end_of_a_task_removes_its_pins_and_deletes_the_ci_branch_only_when_allowed() {
    for forbid in [false, true] {
        let f = fixture();
        let task = dirty_task(&f);
        report(&f, &task.id);
        wait_for(|| calls(&f) == 1);
        f.ci.finish(passed());
        f.settle();
        let ci_branch = format!("natamux-ci/{}", task.branch.clone().unwrap());
        let target = format!("HEAD:refs/heads/{ci_branch}");
        run_git(&worktree(&task), &["push", "-q", "origin", &target]);
        let _forbid = forbid.then(|| force(strict(CiPush::Forbid)));

        f.apply(
            &task.id,
            json!({ "source": "human", "type": "task.cancelled" }),
        );
        f.settle();

        let origin = origin_of(&f);
        let left = run_git(&origin, &["for-each-ref", "--format=%(refname)"]);
        assert_eq!(left.contains(&ci_branch), forbid, "forbid: {forbid}");
        let pins = run_git(&f.repo, &["for-each-ref", "refs/natamux/ci/"]);
        assert_eq!(pins, "", "forbid: {forbid}");
    }
}

fn cancel(f: &Fixture, id: &str) {
    f.apply(id, json!({ "source": "human", "type": "task.cancelled" }));
    f.settle();
}

/// The dialog must show what committed work the push adds, workflow files above all.
#[test]
fn the_approval_lists_the_paths_of_new_commits_and_flags_a_workflow_change() {
    let f = fixture();
    let _strict = force(strict(CiPush::ApprovedSnapshot));
    let task = dirty_task(&f);
    let worktree = worktree(&task);
    std::fs::create_dir_all(worktree.join(".github/workflows")).unwrap();
    std::fs::write(worktree.join(".github/workflows/ci.yml"), "on: push\n").unwrap();
    run_git(&worktree, &["add", ".github"]);
    run_git(&worktree, &["commit", "-q", "-m", "workflow"]);
    report(&f, &task.id);
    f.settle();

    let needed = payload(&f, &task.id, "stage.ci-approval-needed");
    let files = needed["commitFiles"].as_array().unwrap();
    assert!(
        files.contains(&json!(".github/workflows/ci.yml")),
        "{needed:?}"
    );
    assert_eq!(needed["workflowsChanged"], true);
    assert!(needed["newCommits"].as_u64().unwrap() >= 1);
    assert!(needed["commitFilesCount"].as_u64().unwrap() >= files.len() as u64);
}

#[test]
fn an_approval_after_the_task_ended_is_refused_and_pushes_nothing() {
    let f = fixture();
    let _strict = force(strict(CiPush::ApprovedSnapshot));
    let task = dirty_task(&f);
    report(&f, &task.id);
    f.settle();
    let needed = payload(&f, &task.id, "stage.ci-approval-needed");
    let tree = needed["tree"].as_str().unwrap().to_string();
    cancel(&f, &task.id);

    let error = f.runner.run.approve_ci(&task.id, "implement", &tree);

    assert!(error.unwrap_err().to_string().contains("has ended"));
    f.settle();
    assert_eq!(calls(&f), 0);
    assert!(f.last_of(&task.id, "stage.ci-approved").is_none());
}

#[test]
fn the_end_of_a_task_deletes_the_ci_branch_of_every_round() {
    let f = fixture();
    let task = f.implementing("implement");
    for ci_branch in ["natamux-ci/a", "natamux-ci/b"] {
        let target = format!("HEAD:refs/heads/{ci_branch}");
        run_git(&worktree(&task), &["push", "-q", "origin", &target]);
        let approved = json!({ "stage": "implement", "branch": ci_branch });
        f.tasks
            .record_event(&task.id, "stage.ci-approved", approved, "system")
            .unwrap();
    }

    cancel(&f, &task.id);

    let left = run_git(&origin_of(&f), &["for-each-ref", "--format=%(refname)"]);
    assert!(!left.contains("natamux-ci/"), "{left}");
}

#[test]
fn a_task_whose_ci_was_turned_off_after_a_round_leaves_no_pins() {
    let f = fixture();
    let task = dirty_task(&f);
    report(&f, &task.id);
    wait_for(|| calls(&f) == 1);
    f.ci.finish(passed());
    f.settle();
    f.set_ci(&task.id, "implement", false);

    cancel(&f, &task.id);

    let pins = format!("refs/natamux/ci/{}/", task.id);
    assert_eq!(run_git(&f.repo, &["for-each-ref", &pins]), "");
}
