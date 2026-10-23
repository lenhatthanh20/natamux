use super::*;

fn commit(hash: &str, parents: &[&str]) -> GraphCommit {
    GraphCommit {
        hash: hash.into(),
        parents: parents.iter().map(|p| p.to_string()).collect(),
        author: "Ann".into(),
        date: "2026-10-08T09:00:00.000Z".into(),
        subject: hash.into(),
        ..GraphCommit::default()
    }
}

fn row(lane: usize, through: &[usize], into: &[usize], out: &[usize]) -> GraphRow {
    GraphRow {
        lane,
        through: through.to_vec(),
        into: into.to_vec(),
        out: out.to_vec(),
    }
}

#[test]
fn keeps_a_straight_history_in_one_lane() {
    let (rows, width) = layout_graph(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
    assert_eq!(width, 1);
    assert_eq!(
        rows,
        [
            row(0, &[], &[], &[0]),
            row(0, &[], &[0], &[0]),
            row(0, &[], &[0], &[])
        ]
    );
}

#[test]
fn opens_a_second_lane_for_a_branch_and_joins_it_at_the_fork() {
    // task: t2 -> t1 -> base; main: m1 -> base
    let (rows, width) = layout_graph(&[
        commit("t2", &["t1"]),
        commit("m1", &["base"]),
        commit("t1", &["base"]),
        commit("base", &[]),
    ]);
    assert_eq!(width, 2);
    assert_eq!(
        rows,
        [
            row(0, &[], &[], &[0]),
            row(1, &[0], &[], &[1]),
            row(0, &[1], &[0], &[1]),
            row(1, &[], &[1], &[])
        ]
    );
}

#[test]
fn draws_a_merge_commit_with_a_line_to_each_parent() {
    let (rows, width) = layout_graph(&[
        commit("m", &["a", "b"]),
        commit("b", &["base"]),
        commit("a", &["base"]),
        commit("base", &[]),
    ]);
    assert_eq!(width, 2);
    assert_eq!(
        rows,
        [
            row(0, &[], &[], &[0, 1]),
            row(1, &[0], &[1], &[1]),
            row(0, &[1], &[0], &[1]),
            row(1, &[], &[1], &[])
        ]
    );
}

#[test]
fn reuses_a_lane_once_it_is_free() {
    let (rows, width) = layout_graph(&[
        commit("x", &["a"]),
        commit("y", &["b"]),
        commit("a", &[]),
        commit("z", &["b"]),
        commit("b", &[]),
    ]);
    let lanes: Vec<usize> = rows.iter().map(|row| row.lane).collect();
    assert_eq!(lanes, [0, 1, 0, 0, 1]);
    assert_eq!(width, 2);
}

#[test]
fn keeps_a_lane_open_for_a_parent_that_is_not_in_the_list() {
    let (rows, _) = layout_graph(&[commit("b", &["a"])]);
    assert_eq!(rows[0].out, [0]);
}

#[test]
fn has_no_rows_and_no_width_without_commits() {
    assert_eq!(layout_graph(&[]), (Vec::new(), 0));
}

fn graph(base: Option<&str>, ahead: Option<u64>, behind: Option<u64>) -> GitGraph {
    GitGraph {
        base: base.map(String::from),
        ahead,
        behind,
        ..GitGraph::default()
    }
}

#[test]
fn says_when_there_is_no_base_branch() {
    let none = "No base branch to compare with.";
    assert_eq!(ahead_label(&graph(None, None, None)), none);
    assert_eq!(ahead_label(&graph(Some("main"), None, Some(1))), none);
    assert_eq!(ahead_label(&graph(Some("main"), Some(1), None)), none);
}

#[test]
fn counts_the_commits_ahead_and_behind() {
    assert_eq!(
        ahead_label(&graph(Some("main"), Some(1), Some(0))),
        "1 commit ahead of main, 0 behind."
    );
    assert_eq!(
        ahead_label(&graph(Some("main"), Some(3), Some(2))),
        "3 commits ahead of main, 2 behind."
    );
    assert_eq!(
        ahead_label(&graph(Some("dev"), Some(0), Some(1))),
        "0 commits ahead of dev, 1 behind."
    );
}

#[test]
fn names_the_files_not_committed_and_the_truncated_history() {
    assert_eq!(uncommitted_label(1), "1 file not committed.");
    assert_eq!(uncommitted_label(4), "4 files not committed.");
    assert_eq!(
        truncated_label(200),
        " Only the newest 200 commits are shown."
    );
}

#[test]
fn shows_two_refs_and_counts_the_rest() {
    let refs = |names: &[&str]| -> Vec<String> { names.iter().map(|n| n.to_string()).collect() };
    let none = refs(&[]);
    let (shown, more) = shown_refs(&none);
    assert!(shown.is_empty());
    assert_eq!(more, None);
    let two = refs(&["a", "b"]);
    assert_eq!(shown_refs(&two), (&two[..], None));
    let four = refs(&["a", "b", "c", "d"]);
    assert_eq!(shown_refs(&four), (&four[..2], Some("+2".to_string())));
    assert_eq!(hidden_refs_tip(&four), "c, d");
}
