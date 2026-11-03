use serde_json::{Value, json};

use super::*;
use crate::sidebar::workspace_groups::RepoRef;

fn make_task(overrides: Value) -> Task {
    let mut base = json!({
        "id": "T-0001", "title": "Example", "state": "running", "workspace": "/work/T-0001",
        "createdAt": "2026-10-08T09:00:00.000Z", "updatedAt": "2026-10-08T09:00:00.000Z"
    });
    if let (Some(base), Value::Object(extra)) = (base.as_object_mut(), overrides) {
        base.extend(extra);
    }
    serde_json::from_value(base).unwrap_or_default()
}

fn task(id: &str, title: &str) -> Task {
    make_task(json!({"id": id, "title": title}))
}

fn group(id: &str, open: Vec<Task>, finished: Vec<Task>) -> RepoGroup {
    RepoGroup {
        repo: RepoRef {
            id: id.into(),
            name: format!("Repo {id}"),
        },
        open,
        finished,
        nested: Vec::new(),
    }
}

fn shell(key: &str, label: &str) -> ShellRow {
    ShellRow {
        key: key.into(),
        label: label.into(),
        agent: None,
        state: None,
        shell: None,
        plain: false,
    }
}

fn keys(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|each| each.key.as_str()).collect()
}

#[test]
fn lists_pinned_rows_then_each_group_with_its_open_task_shells_and_children() {
    let one = task("T-0001", "One");
    let two = task("T-0002", "Two");
    let child = task("T-0003", "Child");
    let pinned = task("T-0009", "Pinned");
    let children = BTreeMap::from([("T-0001".to_owned(), vec![child.clone()])]);
    let groups = [group("a", vec![one, two], vec![])];
    let source = TreeSource {
        pinned: &[pinned],
        groups: &groups,
        children: &children,
        repo_open: &|_| true,
        finished_shown: &|_| false,
        task_open: &|task| task.id == "T-0001",
        rows_of: &|_| vec![shell("shell:1", "Implement")],
        waiting_children: &|_| Vec::new(),
    };

    let entries = tree_entries(&source);

    assert_eq!(
        keys(&entries),
        [
            "task:T-0009",
            "repo:a",
            "task:T-0001",
            "agent:T-0001:shell:1",
            "task:T-0003",
            "task:T-0002"
        ]
    );
    let parents: Vec<Option<&str>> = entries.iter().map(|each| each.parent.as_deref()).collect();
    assert_eq!(
        parents,
        [
            None,
            None,
            Some("repo:a"),
            Some("task:T-0001"),
            Some("task:T-0001"),
            Some("repo:a")
        ]
    );
    // Every card folds: the pinned one is closed, the open one folds.
    let fold = |id: &str, open: bool| {
        Some(EntryAction::SetFold {
            key: format!("task:{id}"),
            open,
        })
    };
    assert_eq!(entries[0].toggle, fold("T-0009", true));
    assert_eq!(entries[2].toggle, fold("T-0001", false));
    assert!(entries[0].expandable && entries[2].expandable);
    // Children inside the card are plain entries.
    assert!(!entries[4].expandable);
    assert_eq!(entries[4].toggle, None);
    assert_eq!(
        entries[1].toggle,
        Some(EntryAction::SetFold {
            key: "repo:a".into(),
            open: false
        })
    );
}

#[test]
fn lists_only_the_waiting_children_under_a_folded_task() {
    let one = task("T-0001", "One");
    let waiting = task("T-0003", "Waiting");
    let children = BTreeMap::from([("T-0001".to_owned(), vec![task("T-0004", "Hidden")])]);
    let groups = [group("a", vec![one], vec![])];
    let source = TreeSource {
        pinned: &[],
        groups: &groups,
        children: &children,
        repo_open: &|_| true,
        finished_shown: &|_| false,
        task_open: &|_| false,
        rows_of: &|_| vec![shell("shell:1", "Implement")],
        waiting_children: &|_| vec![waiting.clone()],
    };

    assert_eq!(
        keys(&tree_entries(&source)),
        ["repo:a", "task:T-0001", "task:T-0003"]
    );
}

#[test]
fn hides_the_tasks_of_a_folded_group_and_lists_finished_ones_only_when_shown() {
    let groups = [
        group(
            "a",
            vec![task("T-0001", "One")],
            vec![task("T-0002", "Done")],
        ),
        group("b", vec![task("T-0003", "Three")], vec![]),
    ];
    let children = BTreeMap::new();
    let build = |repo_open: &dyn Fn(&str) -> bool, finished_shown: &dyn Fn(&str) -> bool| {
        let source = TreeSource {
            pinned: &[],
            groups: &groups,
            children: &children,
            repo_open,
            finished_shown,
            task_open: &|_| true,
            rows_of: &|_| Vec::new(),
            waiting_children: &|_| Vec::new(),
        };
        tree_entries(&source)
    };

    let folded = build(&|id| id == "a", &|_| false);
    assert_eq!(
        keys(&folded),
        ["repo:a", "task:T-0001", "finished:a", "repo:b"]
    );
    assert_eq!(folded[2].parent.as_deref(), Some("repo:a"));
    assert_eq!(
        folded[2].toggle,
        Some(EntryAction::SetFold {
            key: "finished:a".into(),
            open: true
        })
    );
    let shown = build(&|id| id == "a", &|_| true);
    assert_eq!(
        keys(&shown),
        [
            "repo:a",
            "task:T-0001",
            "finished:a",
            "task:T-0002",
            "repo:b"
        ]
    );
    assert_eq!(shown[3].parent.as_deref(), Some("finished:a"));
}

fn fold_rows(repo_open: bool) -> Vec<Entry> {
    let toggle = EntryAction::SetFold {
        key: "repo:a".into(),
        open: !repo_open,
    };
    vec![
        Entry {
            key: "repo:a".into(),
            parent: None,
            expandable: true,
            label: "A".into(),
            open: Some(repo_open),
            toggle: Some(toggle),
        },
        Entry {
            key: "task:1".into(),
            ..entry("task:1".into(), Some("repo:a"), "One")
        },
        Entry {
            key: "agent:1".into(),
            ..entry("agent:1".into(), Some("task:1"), "Agent")
        },
    ]
}

#[test]
fn right_unfolds_a_closed_group() {
    assert_eq!(
        tree_key("ArrowRight", "repo:a", &fold_rows(false)),
        TreeKey::Toggle(EntryAction::SetFold {
            key: "repo:a".into(),
            open: true
        })
    );
}

#[test]
fn right_steps_into_the_first_child_of_an_open_group() {
    assert_eq!(
        tree_key("ArrowRight", "repo:a", &fold_rows(true)),
        TreeKey::Focus("task:1".into())
    );
}

#[test]
fn right_does_nothing_on_a_row_that_cannot_fold_but_still_handles_the_key() {
    assert_eq!(
        tree_key("ArrowRight", "task:1", &fold_rows(true)),
        TreeKey::Ignored
    );
    let mut rows = fold_rows(true);
    rows.truncate(1);
    assert_eq!(tree_key("ArrowRight", "repo:a", &rows), TreeKey::Handled);
}

#[test]
fn left_folds_an_open_group() {
    assert_eq!(
        tree_key("ArrowLeft", "repo:a", &fold_rows(true)),
        TreeKey::Toggle(EntryAction::SetFold {
            key: "repo:a".into(),
            open: false
        })
    );
}

#[test]
fn left_steps_to_the_parent_of_a_closed_or_plain_row() {
    assert_eq!(
        tree_key("ArrowLeft", "task:1", &fold_rows(true)),
        TreeKey::Focus("repo:a".into())
    );
    assert_eq!(
        tree_key("ArrowLeft", "agent:1", &fold_rows(true)),
        TreeKey::Focus("task:1".into())
    );
    let mut rows = fold_rows(false);
    rows[0].parent = Some("repo:root".into());
    assert_eq!(
        tree_key("ArrowLeft", "repo:a", &rows),
        TreeKey::Focus("repo:root".into())
    );
}

#[test]
fn left_on_a_top_row_that_is_closed_is_left_to_the_list() {
    assert_eq!(
        tree_key("ArrowLeft", "repo:a", &fold_rows(false)),
        TreeKey::Ignored
    );
}

#[test]
fn left_does_not_fold_an_open_row_that_has_no_toggle() {
    let mut rows = fold_rows(true);
    rows[0].toggle = None;
    assert_eq!(tree_key("ArrowLeft", "repo:a", &rows), TreeKey::Ignored);
}

#[test]
fn other_keys_and_unknown_rows_are_ignored() {
    assert_eq!(
        tree_key("Enter", "repo:a", &fold_rows(true)),
        TreeKey::Ignored
    );
    assert_eq!(
        tree_key("ArrowRight", "repo:zzz", &fold_rows(true)),
        TreeKey::Ignored
    );
}

#[test]
fn home_goes_to_the_first_row_and_end_to_the_last() {
    assert_eq!(move_target("Home", 2, 5, false), Some(0));
    assert_eq!(move_target("End", 2, 5, false), Some(4));
}

#[test]
fn up_and_down_stop_at_the_ends() {
    assert_eq!(move_target("ArrowDown", 4, 5, false), Some(4));
    assert_eq!(move_target("ArrowDown", 1, 5, false), Some(2));
    assert_eq!(move_target("ArrowUp", 0, 5, false), Some(0));
    assert_eq!(move_target("ArrowUp", 3, 5, false), Some(2));
}

#[test]
fn j_and_k_move_only_where_letters_are_on() {
    assert_eq!(move_target("j", 1, 5, true), Some(2));
    assert_eq!(move_target("k", 1, 5, true), Some(0));
    assert_eq!(move_target("j", 1, 5, false), None);
}

#[test]
fn an_empty_list_and_other_keys_do_not_move() {
    assert_eq!(move_target("Home", 0, 0, false), None);
    assert_eq!(move_target("End", 0, 0, false), None);
    assert_eq!(move_target("x", 1, 5, true), None);
}

fn card_entries(open: bool) -> Vec<Entry> {
    let one = task("T-0001", "One");
    let groups = [group("a", vec![one], vec![task("T-0002", "Done")])];
    let children = BTreeMap::from([("T-0001".to_owned(), vec![task("T-0003", "Child")])]);
    let source = TreeSource {
        pinned: &[],
        groups: &groups,
        children: &children,
        repo_open: &|_| true,
        finished_shown: &|_| true,
        task_open: &|_| open,
        rows_of: &|_| vec![shell("shell:1", "Implement")],
        waiting_children: &|_| Vec::new(),
    };
    tree_entries(&source)
}

#[test]
fn right_unfolds_a_closed_card() {
    assert_eq!(
        tree_key("ArrowRight", "task:T-0001", &card_entries(false)),
        TreeKey::Toggle(EntryAction::SetFold {
            key: "task:T-0001".into(),
            open: true
        })
    );
}

#[test]
fn left_folds_an_open_card() {
    assert_eq!(
        tree_key("ArrowLeft", "task:T-0001", &card_entries(true)),
        TreeKey::Toggle(EntryAction::SetFold {
            key: "task:T-0001".into(),
            open: false
        })
    );
}

#[test]
fn right_on_an_open_card_goes_to_its_first_terminal_row() {
    assert_eq!(
        tree_key("ArrowRight", "task:T-0001", &card_entries(true)),
        TreeKey::Focus("agent:T-0001:shell:1".into())
    );
}

#[test]
fn a_finished_row_is_one_plain_line() {
    let entries = card_entries(true);
    let finished = entries.iter().find(|each| each.key == "task:T-0002");
    let finished = finished.unwrap();
    assert!(!finished.expandable);
    assert_eq!(finished.toggle, None);
    assert_eq!(
        tree_key("ArrowRight", "task:T-0002", &entries),
        TreeKey::Ignored
    );
}
