use natamux_git::test_support::{bare_origin, run_git, test_repo};
use serde_json::json;

use super::*;

fn stored(seq: u64, kind: &str, payload: Value) -> StoredEvent {
    serde_json::from_value(json!({
        "seq": seq,
        "accepted": true,
        "event": {"taskId": "T-1", "source": "system", "type": kind, "at": "x", "payload": payload}
    }))
    .unwrap()
}

fn object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// A repository with a committed file, a change to it, an untracked fake secret and an ignored
/// file.
fn dirty_repo() -> (natamux_git::test_support::TempDir, std::path::PathBuf) {
    let (root, dir) = test_repo();
    std::fs::write(dir.join("tracked.rs"), "fn old() {}\n").unwrap();
    std::fs::write(dir.join(".gitignore"), "ignored.log\n").unwrap();
    run_git(&dir, &["add", "."]);
    run_git(&dir, &["commit", "-q", "-m", "files"]);
    std::fs::write(dir.join("tracked.rs"), "fn new() {}\n").unwrap();
    std::fs::write(dir.join("secret.txt"), "FAKE-SECRET\n").unwrap();
    std::fs::write(dir.join("ignored.log"), "x\n").unwrap();
    (root, dir)
}

#[test]
fn the_default_approval_holds_the_tracked_change_and_names_the_untracked_file() {
    let (root, dir) = dirty_repo();
    bare_origin(root.path());

    let payload = build_approval(&dir, "T-1", "implement", "t-1", CiMode::Snapshot).unwrap();

    assert_eq!(payload["mode"], "snapshot");
    assert_eq!(payload["branch"], "natamux-ci/t-1");
    assert_eq!(payload["files"], json!(["tracked.rs"]));
    assert_eq!(payload["notSent"], json!(["secret.txt"]));
    assert_eq!(payload["notSentCount"], 1);
    let commit = payload["commit"].as_str().unwrap();
    let names = run_git(&dir, &["ls-tree", "-r", "--name-only", commit]);
    assert!(names.contains("tracked.rs") && !names.contains("secret.txt"));
    assert!(!names.contains("ignored.log"));
    let tree = run_git(&dir, &["rev-parse", &format!("{commit}^{{tree}}")]);
    assert_eq!(payload["tree"], tree.trim());
    // The pin keeps the commit until the task ends.
    let pinned = run_git(&dir, &["rev-parse", "refs/natamux/ci/T-1/implement"]);
    assert_eq!(pinned.trim(), commit);
    assert!(payload["remote"].as_str().unwrap().ends_with("origin.git"));
    assert_eq!(
        not_sent_note(&payload).unwrap(),
        "1 untracked file is not sent: secret.txt"
    );
}

#[test]
fn approved_commit_is_head_and_counts_every_uncommitted_file() {
    let (_root, dir) = dirty_repo();

    let payload = build_approval(&dir, "T-1", "implement", "t-1", CiMode::ApprovedCommit).unwrap();

    let head = run_git(&dir, &["rev-parse", "HEAD"]);
    assert_eq!(payload["commit"], head.trim());
    assert_eq!(payload["files"], json!([]));
    assert_eq!(payload["notSentCount"], 2);
    // No origin: the URL is empty, and the push would fail on its own.
    assert_eq!(payload["remote"], "");
    let short: String = head.trim().chars().take(7).collect();
    assert_eq!(
        not_sent_note(&payload).unwrap(),
        format!("CI checks the last commit ({short}). 2 uncommitted files are not sent.")
    );
}

#[test]
fn the_note_is_none_without_untracked_files_and_cuts_a_long_list() {
    let none = object(json!({ "mode": "snapshot", "notSent": [], "notSentCount": 0 }));
    assert_eq!(not_sent_note(&none), None);
    let many = object(json!({ "mode": "snapshot", "notSent": ["a", "b"], "notSentCount": 5 }));
    assert_eq!(
        not_sent_note(&many).unwrap(),
        "5 untracked files are not sent: a, b, …"
    );
}

#[test]
fn a_pending_approval_ends_with_its_answer_or_a_new_start() {
    let needed = json!({ "stage": "implement", "tree": "t1" });
    let events = [
        stored(1, "stage.ci-approval-needed", needed.clone()),
        stored(
            2,
            "stage.ci-approval-needed",
            json!({ "stage": "testing", "tree": "t2" }),
        ),
    ];
    assert_eq!(
        pending_approval(&events, "implement").unwrap()["tree"],
        "t1"
    );
    assert_eq!(pending_approval(&events, "docs"), None);
    for end in ["stage.ci-approved", "stage.started", "stage.check-failed"] {
        let ended = [
            stored(1, "stage.ci-approval-needed", needed.clone()),
            stored(2, end, json!({ "stage": "implement" })),
        ];
        assert_eq!(pending_approval(&ended, "implement"), None, "{end}");
    }
}

#[test]
fn the_ci_branches_are_those_of_every_round_and_none_without_a_round() {
    let events = [
        stored(1, "stage.ci-approved", json!({ "branch": "natamux-ci/a" })),
        stored(2, "stage.ci-started", json!({ "branch": "natamux-ci/a" })),
        stored(3, "stage.ci-approved", json!({ "branch": "natamux-ci/b" })),
        stored(4, "stage.ci-started", json!({ "branch": "natamux-ci/" })),
        stored(5, "stage.started", json!({ "branch": "main" })),
    ];
    assert_eq!(
        ci_branches_of(&events, Some("c")),
        ["natamux-ci/a", "natamux-ci/b", "natamux-ci/c"]
    );
    assert!(ci_branches_of(&events[4..], Some("c")).is_empty());
}

#[test]
fn cleaning_removes_the_pins_and_never_calls_the_remote_under_forbid() {
    let (root, dir) = dirty_repo();
    let remote = bare_origin(root.path());
    let head = run_git(&dir, &["rev-parse", "HEAD"]);
    let target = format!("{}:refs/heads/natamux-ci/t-1", head.trim());
    run_git(&dir, &["push", "-q", "origin", &target]);
    pin(&dir, "T-1", "implement", head.trim()).unwrap();

    let refs = ["natamux-ci/t-1".to_string()];
    clean_ci_refs(&dir, "T-1", &refs, true);

    assert_eq!(run_git(&dir, &["for-each-ref", "refs/natamux/ci/"]), "");
    let left = run_git(&remote, &["for-each-ref", "--format=%(refname)"]);
    assert_eq!(left.trim(), "refs/heads/natamux-ci/t-1");

    clean_ci_refs(&dir, "T-1", &refs, false);
    assert_eq!(run_git(&remote, &["for-each-ref"]), "");
}
