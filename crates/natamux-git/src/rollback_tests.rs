use std::path::PathBuf;

use super::*;
use crate::commit_push::snapshot_commit;
use crate::test_support::{TempDir, run_git, test_repo};

struct Repo {
    root: TempDir,
    dir: PathBuf,
}

impl Repo {
    fn write(&self, path: &str, text: &str) {
        let file = self.dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(file, text).unwrap();
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.join(path)).unwrap()
    }

    fn exists(&self, path: &str) -> bool {
        self.dir.join(path).exists()
    }

    fn remove(&self, path: &str) {
        std::fs::remove_file(self.dir.join(path)).unwrap();
    }

    // `git status` stands for `listChanges`: a rename is `R` with its old path, a file git does not
    // track is `U`.
    fn changes(&self) -> Vec<ChangeEntry> {
        let out = run_git(&self.dir, &["status", "--porcelain=v1", "-z", "-uall"]);
        let mut records = out.split('\0').filter(|record| !record.is_empty());
        let mut entries = Vec::new();
        while let Some(record) = records.next() {
            let (code, path) = record.split_at(3);
            let letter = if code.starts_with("??") {
                "U"
            } else if code.contains('R') {
                "R"
            } else if code.starts_with('A') {
                "A"
            } else if code.contains('D') {
                "D"
            } else {
                "M"
            };
            let original_path = if letter == "R" {
                records.next().map(str::to_string)
            } else {
                None
            };
            entries.push(entry(path, letter, original_path.as_deref()));
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries
    }

    fn statuses(&self) -> Vec<String> {
        self.changes()
            .iter()
            .map(|entry| format!("{} {}", entry.path, entry.status.letter()))
            .collect()
    }

    // The entries of the given paths, as listChanges gave them.
    fn changes_of(&self, paths: &[&str]) -> Vec<ChangeEntry> {
        let mut entries = self.changes();
        entries.retain(|entry| paths.contains(&entry.path.as_str()));
        entries
    }
}

fn entry(path: &str, status: &str, original: Option<&str>) -> ChangeEntry {
    ChangeEntry {
        path: path.into(),
        status: ChangeStatus::parse(status),
        original_path: original.map(str::to_string),
        added: None,
        removed: None,
    }
}

fn repo() -> Repo {
    let (root, dir) = test_repo();
    // Git for Windows turns LF into CRLF on checkout, and tests compare the bytes of files.
    run_git(&dir, &["config", "core.autocrlf", "false"]);
    let repo = Repo { root, dir };
    repo.write("mod.txt", "m\n");
    repo.write("del.txt", "d\n");
    repo.write("old.txt", "old\n");
    // As a pattern, `[ab].txt` would match a.txt too.
    repo.write("[ab].txt", "ab\n");
    repo.write("a.txt", "a\n");
    run_git(&repo.dir, &["add", "--all"]);
    run_git(&repo.dir, &["commit", "-m", "files"]);
    repo
}

// One change of each kind: modified, deleted, renamed and edited, added, not tracked.
fn change_everything(repo: &Repo) {
    repo.write("mod.txt", "m2\n");
    repo.remove("del.txt");
    run_git(&repo.dir, &["mv", "old.txt", "new.txt"]);
    repo.write("new.txt", "old\nx\n");
    repo.write("added.txt", "add\n");
    run_git(&repo.dir, &["add", "added.txt"]);
    repo.write("sub/deep/u.txt", "u\n");
}

#[test]
fn puts_each_kind_of_change_back_as_it_is_in_head() {
    let repo = repo();
    change_everything(&repo);
    assert_eq!(
        repo.statuses(),
        [
            "added.txt A",
            "del.txt D",
            "mod.txt M",
            "new.txt R",
            "sub/deep/u.txt U"
        ]
    );

    rollback_files(&repo.dir, &repo.changes()).unwrap();

    assert!(repo.changes().is_empty());
    assert_eq!(repo.read("mod.txt"), "m\n");
    assert_eq!(repo.read("del.txt"), "d\n");
    assert_eq!(repo.read("old.txt"), "old\n");
    assert!(!repo.exists("new.txt"));
    assert!(!repo.exists("added.txt"));
    assert!(!repo.exists("sub/deep/u.txt"));
}

#[test]
fn touches_only_the_files_it_is_given() {
    let repo = repo();
    change_everything(&repo);

    let given = repo.changes_of(&["mod.txt", "sub/deep/u.txt"]);
    rollback_files(&repo.dir, &given).unwrap();

    assert_eq!(repo.statuses(), ["added.txt A", "del.txt D", "new.txt R"]);
}

#[test]
fn takes_a_file_name_as_itself_not_as_a_pattern() {
    let repo = repo();
    repo.write("[ab].txt", "ab2\n");
    repo.write("a.txt", "a2\n");
    repo.write("[cd].txt", "new\n");
    repo.write("c.txt", "new\n");

    let given = repo.changes_of(&["[ab].txt", "[cd].txt"]);
    rollback_files(&repo.dir, &given).unwrap();

    assert_eq!(repo.statuses(), ["a.txt M", "c.txt U"]);
    assert_eq!(repo.read("[ab].txt"), "ab\n");
}

#[test]
fn names_both_paths_of_a_rename() {
    let entries = [
        entry("new.txt", "R", Some("old.txt")),
        entry("a.txt", "M", None),
    ];
    assert_eq!(rollback_paths(&entries), ["old.txt", "new.txt", "a.txt"]);
}

#[test]
fn brings_back_every_file_as_it_was_before_the_rollback() {
    let repo = repo();
    change_everything(&repo);
    let entries = repo.changes();
    let snapshot = snapshot_commit(&repo.dir, "Natamux rollback").unwrap();
    rollback_files(&repo.dir, &entries).unwrap();

    restore_from_snapshot(&repo.dir, &snapshot, &rollback_paths(&entries)).unwrap();

    assert_eq!(repo.read("mod.txt"), "m2\n");
    assert_eq!(repo.read("new.txt"), "old\nx\n");
    assert_eq!(repo.read("added.txt"), "add\n");
    assert_eq!(repo.read("sub/deep/u.txt"), "u\n");
    assert!(!repo.exists("del.txt"));
    assert!(!repo.exists("old.txt"));
    // Only the files come back: the added file and the new name are not in the index.
    assert_eq!(
        repo.statuses(),
        [
            "added.txt U",
            "del.txt D",
            "mod.txt M",
            "new.txt U",
            "old.txt D",
            "sub/deep/u.txt U"
        ]
    );
}

// Windows does not allow `:` in a file name.
#[cfg(unix)]
#[test]
fn brings_back_a_file_whose_name_looks_like_a_pattern_and_only_that_file() {
    let repo = repo();
    repo.write(":(top)x", "top\n");
    repo.write("x", "x\n");
    run_git(&repo.dir, &["add", "--all"]);
    run_git(&repo.dir, &["commit", "-m", "magic names"]);
    repo.write(":(top)x", "top2\n");
    repo.write("[ab].txt", "ab2\n");
    let entries = repo.changes_of(&[":(top)x", "[ab].txt"]);
    assert_eq!(entries.len(), 2);
    let snapshot = snapshot_commit(&repo.dir, "Natamux rollback").unwrap();
    rollback_files(&repo.dir, &entries).unwrap();
    assert_eq!(repo.read(":(top)x"), "top\n");

    restore_from_snapshot(&repo.dir, &snapshot, &rollback_paths(&entries)).unwrap();

    assert_eq!(repo.read(":(top)x"), "top2\n");
    assert_eq!(repo.read("[ab].txt"), "ab2\n");
    assert_eq!(repo.read("x"), "x\n");
    assert_eq!(repo.read("a.txt"), "a\n");
}

#[test]
fn leaves_the_other_files_alone() {
    let repo = repo();
    repo.write("mod.txt", "m2\n");
    repo.write("a.txt", "a2\n");
    let entries = repo.changes_of(&["mod.txt"]);
    let snapshot = snapshot_commit(&repo.dir, "Natamux rollback").unwrap();
    rollback_files(&repo.dir, &entries).unwrap();
    repo.write("a.txt", "a3\n");

    restore_from_snapshot(&repo.dir, &snapshot, &rollback_paths(&entries)).unwrap();

    assert_eq!(repo.read("mod.txt"), "m2\n");
    assert_eq!(repo.read("a.txt"), "a3\n");
}

#[test]
fn is_false_for_a_repository_without_a_commit() {
    let repo = repo();
    let empty = repo.root.path().join("empty");
    run_git(
        repo.root.path(),
        &["init", "-b", "main", &empty.to_string_lossy()],
    );

    assert!(has_commit(&repo.dir));
    assert!(!has_commit(&empty));
}

#[test]
fn keeps_each_group_of_paths_short_enough_for_one_command_line() {
    let long = "x".repeat(40_000);
    assert!(path_batches(&[]).is_empty());
    assert_eq!(
        path_batches(&["a".to_string(), "b".to_string()]),
        [["a", "b"]]
    );
    assert_eq!(
        path_batches(&[long.clone(), long.clone(), "a".to_string()]),
        vec![vec![long.clone()], vec![long, "a".to_string()]]
    );
}
