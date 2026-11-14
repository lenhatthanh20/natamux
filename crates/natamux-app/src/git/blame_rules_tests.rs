use natamux_proto::workspaces::changes::{BlameRun, UNCOMMITTED_SHA};

use super::*;

fn commit(sha: &str, author: &str, summary: &str) -> BlameCommit {
    BlameCommit {
        sha: sha.into(),
        author: author.into(),
        // 2026-10-30T12:00:00Z
        time: 1_793_361_600,
        summary: summary.into(),
    }
}

fn answer() -> BlameAnswer {
    let run = |start, count, commit| BlameRun {
        start,
        count,
        commit,
    };
    BlameAnswer {
        path: "a.rs".into(),
        tracked: true,
        commits: vec![
            commit(&"a".repeat(40), "Ann", "First"),
            commit(UNCOMMITTED_SHA, "Not Committed Yet", "Version of a.rs"),
            commit(&"b".repeat(40), "Bartholomew Longname", "Second"),
        ],
        runs: vec![run(0, 2, 0), run(2, 1, 1), run(3, 10, 2)],
    }
}

#[test]
fn labels_every_line_with_the_same_width_and_stops_at_the_last_line() {
    let labels = line_labels(&answer(), 5);
    let lines: Vec<u32> = labels.iter().map(|(line, _)| *line).collect();
    assert_eq!(lines, vec![0, 1, 2, 3, 4]);
    assert_eq!(labels[0].1, "2026-10-30 Ann             ");
    assert_eq!(labels[2].1, "Not committed              ");
    assert_eq!(labels[3].1, "2026-10-30 Bartholomew Lo  ");
    assert!(labels.iter().all(|(_, label)| label.chars().count() == 27));
}

#[test]
fn finds_the_commit_of_a_line() {
    let answer = answer();
    assert_eq!(commit_at(&answer, 1).unwrap().author, "Ann");
    assert_eq!(commit_at(&answer, 12).unwrap().summary, "Second");
    assert!(commit_at(&answer, 13).is_none());
}

#[test]
fn the_menu_turns_on_or_shows_the_commit_and_close() {
    assert_eq!(
        menu_items(false, true, None),
        vec![BlameItem::Annotate { blocked: true }]
    );
    let answer = answer();
    let first = commit_at(&answer, 0);
    let items = menu_items(true, false, first);
    assert_eq!(
        items[0],
        BlameItem::Title("aaaaaaa · Ann · 2026-10-30 — First".into())
    );
    assert_eq!(items[1], BlameItem::CopyHash("a".repeat(40)));
    assert_eq!(items[2], BlameItem::Close);
    let uncommitted = commit_at(&answer, 2);
    assert_eq!(menu_items(true, false, uncommitted), vec![BlameItem::Close]);
}

#[test]
fn a_long_title_is_cut_to_80_characters() {
    let long = commit(&"c".repeat(40), "Ann", &"word ".repeat(30));
    let title = menu_title(&long);
    assert_eq!(title.chars().count(), 80);
    assert!(title.ends_with('…'));
}
