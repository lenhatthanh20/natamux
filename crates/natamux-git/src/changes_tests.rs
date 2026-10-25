use super::*;
use crate::test_support::{TempDir, run_git, test_repo};
use crate::worktrees::add_worktree;
use std::time::Duration;

const BRANCH: &str = "natamux/t-0001-example";

// The repository and a worktree of it, both inside the returned folder.
fn repo_and_worktree() -> (TempDir, PathBuf, PathBuf) {
    let (root, repo) = test_repo();
    let path = root.path().join("worktrees").join("T-0001");
    add_worktree(&repo, &path, BRANCH, None, false).unwrap();
    (root, repo, path)
}

fn counted(
    path: &str,
    status: ChangeStatus,
    original: Option<&str>,
    counts: (Option<u64>, Option<u64>),
) -> ChangeEntry {
    ChangeEntry {
        added: counts.0,
        removed: counts.1,
        ..entry(path, status, original.map(str::to_string))
    }
}

#[test]
fn turns_the_codes_into_a_status() {
    use ChangeStatus::*;
    let rows = [
        (' ', 'M', Modified),
        ('M', ' ', Modified),
        ('M', 'M', Modified),
        ('A', ' ', Added),
        (' ', 'D', Deleted),
        ('D', ' ', Deleted),
        ('R', ' ', Renamed),
        ('C', ' ', Added),
        ('?', '?', Untracked),
        ('U', 'U', Conflict),
        ('A', 'A', Conflict),
        ('D', 'D', Conflict),
    ];
    for (index, worktree, status) in rows {
        assert_eq!(
            status_from_codes(index, worktree),
            status,
            "the codes {index:?} and {worktree:?}"
        );
    }
}

#[test]
fn reads_one_entry_for_each_file_and_nothing_for_no_output() {
    assert_eq!(parse_status(""), vec![]);
    assert_eq!(
        parse_status(" M a.txt\0?? new file.txt\0A  added.ts\0 D gone.ts\0"),
        vec![
            entry("a.txt", ChangeStatus::Modified, None),
            entry("new file.txt", ChangeStatus::Untracked, None),
            entry("added.ts", ChangeStatus::Added, None),
            entry("gone.ts", ChangeStatus::Deleted, None),
        ]
    );
}

#[test]
fn reads_the_old_path_that_follows_a_renamed_file_and_goes_on_after_it() {
    assert_eq!(
        parse_status("R  new.txt\0old.txt\0 M other.txt\0"),
        vec![
            entry(
                "new.txt",
                ChangeStatus::Renamed,
                Some("old.txt".to_string())
            ),
            entry("other.txt", ChangeStatus::Modified, None),
        ]
    );
}

#[test]
fn marks_a_conflict() {
    assert_eq!(
        parse_status("UU c.txt\0"),
        vec![entry("c.txt", ChangeStatus::Conflict, None)]
    );
}

fn worktree_with_tracked_file() -> (TempDir, PathBuf, PathBuf) {
    let (root, repo, path) = repo_and_worktree();
    std::fs::write(path.join("tracked.txt"), "one\n").unwrap();
    run_git(&path, &["add", "tracked.txt"]);
    run_git(&path, &["commit", "-m", "add tracked"]);
    (root, repo, path)
}

#[test]
fn lists_nothing_for_a_worktree_that_was_not_touched() {
    let (_root, _repo, path) = worktree_with_tracked_file();
    assert_eq!(list_changes(&path, false).unwrap(), vec![]);
}

#[test]
fn lists_modified_added_deleted_and_untracked_files_sorted_by_path() {
    let (_root, _repo, path) = worktree_with_tracked_file();
    std::fs::write(path.join("tracked.txt"), "two\n").unwrap();
    std::fs::write(path.join("untracked.txt"), "new\n").unwrap();
    std::fs::write(path.join("staged.txt"), "staged\n").unwrap();
    run_git(&path, &["add", "staged.txt"]);

    assert_eq!(
        list_changes(&path, false).unwrap(),
        vec![
            entry("staged.txt", ChangeStatus::Added, None),
            entry("tracked.txt", ChangeStatus::Modified, None),
            entry("untracked.txt", ChangeStatus::Untracked, None),
        ]
    );

    std::fs::remove_file(path.join("tracked.txt")).unwrap();
    let changes = list_changes(&path, false).unwrap();
    let deleted = changes.iter().find(|change| change.path == "tracked.txt");
    assert_eq!(
        deleted,
        Some(&entry("tracked.txt", ChangeStatus::Deleted, None))
    );
}

#[test]
fn shows_a_renamed_file_with_its_old_path() {
    let (_root, _repo, path) = worktree_with_tracked_file();
    run_git(&path, &["mv", "tracked.txt", "moved.txt"]);

    assert_eq!(
        list_changes(&path, false).unwrap(),
        vec![entry(
            "moved.txt",
            ChangeStatus::Renamed,
            Some("tracked.txt".to_string())
        )]
    );
}

#[test]
fn reads_plain_renamed_and_binary_records_keyed_by_the_path_now() {
    let output = [
        "3\t1\tsrc/a.ts",
        "0\t0\t",
        "old.ts",
        "new.ts",
        "-\t-\tlogo.png",
        "",
    ]
    .join("\0");

    let expected = HashMap::from([
        (
            "src/a.ts".to_string(),
            LineCounts {
                added: Some(3),
                removed: Some(1),
            },
        ),
        (
            "new.ts".to_string(),
            LineCounts {
                added: Some(0),
                removed: Some(0),
            },
        ),
        (
            "logo.png".to_string(),
            LineCounts {
                added: None,
                removed: None,
            },
        ),
    ]);
    assert_eq!(parse_numstat_z(&output), expected);
    assert_eq!(parse_numstat_z(""), HashMap::new());
}

#[test]
fn counts_line_breaks_and_a_last_line_without_one() {
    assert_eq!(count_text_lines(b""), 0);
    assert_eq!(count_text_lines(b"a\nb\n"), 2);
    assert_eq!(count_text_lines(b"a\nb"), 2);
    assert_eq!(count_text_lines(b"\n"), 1);
}

#[test]
fn counts_the_lines_of_modified_deleted_untracked_and_binary_files() {
    let (_root, _repo, path) = repo_and_worktree();
    std::fs::write(path.join("tracked.txt"), "one\n").unwrap();
    std::fs::write(path.join("gone.txt"), "a\nb\n").unwrap();
    std::fs::write(path.join("data.bin"), [1u8, 0, 2]).unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "add files"]);

    std::fs::write(path.join("tracked.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::remove_file(path.join("gone.txt")).unwrap();
    std::fs::write(path.join("data.bin"), [1u8, 0, 3]).unwrap();
    std::fs::write(path.join("new.txt"), "a\nb\nc").unwrap();
    std::fs::write(path.join("new.bin"), [0u8, 1]).unwrap();

    let (none, modified) = ((None, None), ChangeStatus::Modified);
    assert_eq!(
        list_changes(&path, true).unwrap(),
        vec![
            counted("data.bin", modified, None, none),
            counted("gone.txt", ChangeStatus::Deleted, None, (Some(0), Some(2))),
            counted("new.bin", ChangeStatus::Untracked, None, none),
            counted("new.txt", ChangeStatus::Untracked, None, (Some(3), Some(0))),
            counted("tracked.txt", modified, None, (Some(2), Some(0))),
        ]
    );
}

#[test]
fn still_lists_the_changes_when_a_new_folder_is_a_nested_repository() {
    let (_root, _repo, path) = repo_and_worktree();
    std::fs::write(path.join("new.txt"), "a\n").unwrap();
    let nested = path.join("sub");
    std::fs::create_dir(&nested).unwrap();
    run_git(&nested, &["init", "-b", "main"]);
    run_git(&nested, &["commit", "--allow-empty", "-m", "init"]);

    let changes = list_changes(&path, true).unwrap();

    let untracked = ChangeStatus::Untracked;
    assert!(changes.contains(&counted("new.txt", untracked, None, (Some(1), Some(0)))));
    let sub = changes.iter().find(|change| change.path.starts_with("sub"));
    let sub = sub.unwrap();
    assert_eq!(
        (sub.status, sub.added, sub.removed),
        (untracked, None, None)
    );
}

#[test]
fn leaves_the_counts_out_by_default() {
    let (_root, _repo, path) = repo_and_worktree();
    std::fs::write(path.join("new.txt"), "a\n").unwrap();

    let changes = list_changes(&path, false).unwrap();
    let first = changes.first().unwrap();

    assert_eq!((first.added, first.removed), (None, None));
    // Node leaves the keys out, and the JSON of the change does the same.
    assert_eq!(
        change_json(first, false).to_string(),
        r#"{"path":"new.txt","status":"U","originalPath":null}"#
    );
    assert_eq!(
        change_json(first, true).to_string(),
        r#"{"path":"new.txt","status":"U","originalPath":null,"added":null,"removed":null}"#
    );
}

#[test]
fn sums_numstat_lines_binary_files_without_lines() {
    assert_eq!(
        parse_numstat("3\t1\ta.ts\n-\t-\timage.png\n10\t0\tb.ts\n"),
        DiffStat {
            files: 3,
            added: 13,
            removed: 1
        }
    );
    assert_eq!(
        parse_numstat(""),
        DiffStat {
            files: 0,
            added: 0,
            removed: 0
        }
    );
}

#[test]
fn counts_the_branch_and_uncommitted_changes_from_where_it_left_the_base() {
    let (root, repo) = test_repo();
    std::fs::write(repo.join("base.txt"), "one\ntwo\n").unwrap();
    run_git(&repo, &["add", "base.txt"]);
    run_git(&repo, &["commit", "-m", "base"]);
    let path = root.path().join("worktrees").join("T-0001");
    add_worktree(&repo, &path, BRANCH, None, false).unwrap();
    std::fs::write(path.join("base.txt"), "one\nthree\nfour\n").unwrap();
    run_git(&path, &["commit", "-am", "change"]);
    std::fs::write(path.join("base.txt"), "one\nthree\nfour\nfive\n").unwrap();
    std::fs::write(repo.join("later.txt"), "main moved\n").unwrap();
    run_git(&repo, &["add", "later.txt"]);
    run_git(&repo, &["commit", "-m", "main moved"]);

    assert_eq!(
        read_diff_stat(&path, Some("main")).unwrap(),
        DiffStat {
            files: 1,
            added: 3,
            removed: 1
        }
    );
    assert_eq!(
        read_diff_stat(&path, None).unwrap(),
        DiffStat {
            files: 1,
            added: 1,
            removed: 0
        }
    );
}

#[test]
fn leave_the_index_alone_so_an_agent_can_still_take_its_lock() {
    let (_root, _repo, path) = repo_and_worktree();
    let file = path.join("tracked.txt");
    std::fs::write(&file, "one\n").unwrap();
    run_git(&path, &["add", "tracked.txt"]);
    run_git(&path, &["commit", "-m", "add tracked"]);
    // Same content, new time: git would write the new time back into the index.
    let hour_ago = SystemTime::now() - Duration::from_secs(3600);
    let handle = std::fs::File::options().write(true).open(&file).unwrap();
    handle.set_modified(hour_ago).unwrap();
    drop(handle);
    let index = path.join(run_git(&path, &["rev-parse", "--git-path", "index"]).trim());
    let before = std::fs::read(&index).unwrap();

    assert_eq!(list_changes(&path, true).unwrap(), vec![]);
    assert_eq!(
        read_diff_stat(&path, None).unwrap(),
        DiffStat {
            files: 0,
            added: 0,
            removed: 0
        }
    );
    assert_eq!(
        read_diff_stat(&path, Some("main")).unwrap(),
        DiffStat {
            files: 1,
            added: 1,
            removed: 0
        }
    );
    crate::graph::read_git_graph(&path, Some("main"), 200).unwrap();
    assert_eq!(std::fs::read(&index).unwrap(), before);

    // Proves the file was stat-dirty: a plain status does write the index.
    run_git(&path, &["status"]);
    assert_ne!(std::fs::read(&index).unwrap(), before);
}

#[test]
fn sorts_paths_like_locale_compare() {
    let mut paths = vec![
        "b.txt", "B.txt", "a/b.txt", "a-b.txt", "a1.txt", "a_b.txt", "A.txt",
    ];
    paths.sort_by(|a, b| locale_compare(a, b));
    assert_eq!(
        paths,
        vec![
            "a_b.txt", "a-b.txt", "A.txt", "a/b.txt", "a1.txt", "b.txt", "B.txt"
        ]
    );
}
