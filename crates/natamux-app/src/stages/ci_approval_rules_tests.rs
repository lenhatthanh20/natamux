use serde_json::{Map, Value, json};

use super::*;

fn event(kind: &str, stage: &str, extra: Value) -> TaskEvent {
    let mut payload = Map::new();
    payload.insert("stage".into(), json!(stage));
    if let Value::Object(map) = extra {
        payload.extend(map);
    }
    TaskEvent {
        task_id: "T-1".into(),
        source: "system".into(),
        kind: kind.into(),
        at: "2026-11-23T09:00:00.000Z".into(),
        payload,
    }
}

fn needed(tree: &str) -> Value {
    json!({
        "tree": tree, "branch": "natamux-ci/t-1", "remote": "https://example.com/a.git",
        "files": ["src/a.rs", "src/b.rs"], "notSent": ["notes.txt"], "notSentCount": 3,
        "newCommits": 0, "commitFiles": [], "commitFilesCount": 0, "workflowsChanged": false,
    })
}

#[test]
fn keeps_only_the_latest_unanswered_ask_of_each_stage() {
    let events = [
        event("stage.ci-approval-needed", "implement", needed("aaaaaaa1")),
        event("stage.ci-approval-needed", "implement", needed("bbbbbbb2")),
        event("stage.ci-approval-needed", "testing", needed("ccccccc3")),
        event("stage.ci-approved", "testing", json!({})),
        event("stage.ci-approval-needed", "docs", needed("ddddddd4")),
        event("stage.started", "docs", json!({})),
    ];
    let asks = pending_ci_asks(&events);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].stage, "implement");
    assert_eq!(asks[0].tree, "bbbbbbb2");
    assert_eq!(asks[0].files, ["src/a.rs", "src/b.rs"]);
    assert_eq!(asks[0].not_sent_count, 3);
}

#[test]
fn says_the_tree_the_branch_the_remote_and_what_is_not_sent() {
    let ask = event(
        "stage.ci-approval-needed",
        "implement",
        needed("0123456789abcdef"),
    );
    let asks = pending_ci_asks(&[ask]);
    let text = ask_description(&asks[0]);
    assert!(text.contains("tree 0123456 to natamux-ci/t-1 on https://example.com/a.git"));
    assert!(text.contains("exactly the 2 files"));
    assert!(text.contains("Not sent: notes.txt and 2 more."));
}

#[test]
fn lists_the_files_and_folds_the_rest_into_one_row() {
    let needed = event("stage.ci-approval-needed", "implement", needed("t"));
    let mut ask = pending_ci_asks(&[needed]).remove(0);
    assert_eq!(ask_rows(&ask, 5).len(), 2);
    ask.files = (0..12).map(|n| format!("f{n}")).collect();
    let rows = ask_rows(&ask, 10);
    assert_eq!(rows.len(), 11);
    assert_eq!(rows[10].1, "and 2 more files");
}

fn needed_with(extra: Value) -> TaskEvent {
    let mut ask = event("stage.ci-approval-needed", "implement", needed("t"));
    if let Value::Object(map) = extra {
        ask.payload.extend(map);
    }
    ask
}

#[test]
fn shows_the_paths_of_new_commits_and_warns_about_a_workflow_change() {
    let extra = json!({
        "newCommits": 2, "commitFiles": [".github/workflows/ci.yml", "src/c.rs"],
        "commitFilesCount": 3, "workflowsChanged": true,
    });
    let ask = pending_ci_asks(&[needed_with(extra)]).remove(0);

    let text = ask_description(&ask);
    assert!(text.contains("It also sends 2 commits that no remote has (3 files)."));
    let rows = ask_rows(&ask, 10);
    assert!(rows[0].1.contains(".github/workflows/"), "{rows:?}");
    let titles: Vec<&str> = rows.iter().map(|(_, title)| title.as_str()).collect();
    assert!(titles.contains(&".github/workflows/ci.yml"), "{titles:?}");
    assert_eq!(titles.last(), Some(&"and 1 more files"));
}

#[test]
fn warns_when_the_commits_could_not_be_read() {
    let unknown = json!({ "newCommits": null, "workflowsChanged": null });
    let ask = pending_ci_asks(&[needed_with(unknown)]).remove(0);

    assert!(ask_description(&ask).contains("could not read which commits"));
    assert!(ask_rows(&ask, 10)[0].1.contains("could not check"));
}

#[test]
fn a_finished_task_has_no_ask() {
    let events = [event("stage.ci-approval-needed", "implement", needed("t"))];
    assert_eq!(open_ci_asks(TaskState::Review, &events).len(), 1);
    assert!(open_ci_asks(TaskState::Cancelled, &events).is_empty());
    assert!(open_ci_asks(TaskState::Done, &events).is_empty());
}
