use super::*;
use crate::test_support::{run_git, test_repo};
use natamux_proto::workspaces::changes::UNCOMMITTED_SHA;

const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn run(start: u32, count: u32, commit: usize) -> BlameRun {
    BlameRun {
        start,
        count,
        commit,
    }
}

fn fields(sha: &str, old: u32, new: u32, author: &str, time: i64, summary: &str) -> String {
    format!(
        "{sha} {old} {new} 1\nauthor {author}\nauthor-mail <a@b>\nauthor-time {time}\n\
         author-tz +0000\ncommitter {author}\ncommitter-time {time}\nsummary {summary}\n\
         filename f.txt\n"
    )
}

#[test]
fn reads_runs_and_commits_in_file_order() {
    let text = format!(
        "{}\tone\n{A} 2 2\n\ttwo\n{}\tthree\t\tx\n{A} 4 4\n\tfour\n",
        fields(A, 1, 1, "Ann", 100, "First"),
        fields(B, 3, 3, "Bob", 200, "Second"),
    );
    let (commits, runs) = parse_porcelain(&text);
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].author, "Ann");
    assert_eq!(commits[0].time, 100);
    assert_eq!(commits[0].summary, "First");
    assert_eq!(commits[1].sha, B);
    assert_eq!(runs, vec![run(0, 2, 0), run(2, 1, 1), run(3, 1, 0)]);
}

#[test]
fn sorts_lines_that_git_gives_out_of_order() {
    let text = format!(
        "{}\tthree\n{}\tone\n{B} 2 2\n\ttwo\n",
        fields(A, 3, 3, "Ann", 1, "a"),
        fields(B, 1, 1, "Bob", 2, "b"),
    );
    let (_, runs) = parse_porcelain(&text);
    assert_eq!(runs, vec![run(0, 2, 1), run(2, 1, 0)]);
}

#[test]
fn an_empty_blame_has_no_runs() {
    assert_eq!(parse_porcelain(""), (Vec::new(), Vec::new()));
}

#[test]
fn blames_a_real_file_with_an_uncommitted_line() {
    let (_root, repo) = test_repo();
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    run_git(&repo, &["add", "a.txt"]);
    run_git(&repo, &["commit", "-m", "Add a"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\nthree\n").unwrap();

    let now = read_blame(&repo, "a.txt", false).unwrap();
    assert!(now.tracked);
    assert_eq!(now.runs.len(), 2);
    assert_eq!((now.runs[0].start, now.runs[0].count), (0, 2));
    assert_eq!(now.commits[now.runs[0].commit].summary, "Add a");
    assert_eq!(now.commits[now.runs[1].commit].sha, UNCOMMITTED_SHA);
    assert!(!now.commits[now.runs[1].commit].committed());

    let head = read_blame(&repo, "a.txt", true).unwrap();
    assert_eq!(head.runs, vec![run(0, 2, 0)]);
}

#[test]
fn an_untracked_file_is_not_tracked() {
    let (_root, repo) = test_repo();
    std::fs::write(repo.join("new.txt"), "x\n").unwrap();
    let answer = read_blame(&repo, "new.txt", false).unwrap();
    assert!(!answer.tracked);
    assert!(answer.runs.is_empty());
}
