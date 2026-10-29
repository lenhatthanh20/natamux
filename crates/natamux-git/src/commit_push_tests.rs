use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::test_support::{TempDir, run_git, test_repo};

const BRANCH: &str = "natamux/t-0001-example";

struct Fixture {
    root: TempDir,
    dir: PathBuf,
    worktree: PathBuf,
}

fn fixture() -> Fixture {
    let (root, dir) = test_repo();
    let worktree = root.path().join("worktrees").join("T-0001");
    Fixture {
        root,
        dir,
        worktree,
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn rev(cwd: &Path, name: &str) -> String {
    run_git(cwd, &["rev-parse", name]).trim().to_string()
}

fn status(cwd: &Path) -> String {
    run_git(cwd, &["status", "--porcelain"])
}

fn write(cwd: &Path, name: &str, content: &str) {
    std::fs::write(cwd.join(name), content).unwrap();
}

fn git_succeeds(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
        .status
        .success()
}

// A worktree with a new branch, as `addWorktree` makes it.
fn add_worktree(f: &Fixture) {
    let path = text(&f.worktree);
    run_git(
        &f.dir,
        &["worktree", "add", "--no-track", "-b", BRANCH, &path],
    );
}

fn bare_origin(f: &Fixture) -> PathBuf {
    let remote = f.root.path().join("origin.git");
    let remote_text = text(&remote);
    run_git(
        f.root.path(),
        &["init", "--bare", "-b", "main", &remote_text],
    );
    run_git(&f.dir, &["remote", "add", "origin", &remote_text]);
    remote
}

fn init_empty(f: &Fixture) -> PathBuf {
    let empty = f.root.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    run_git(&empty, &["init", "-b", "main"]);
    empty
}

fn clone_branch(f: &Fixture, remote: &Path) -> PathBuf {
    let other = f.root.path().join("other");
    let (remote_text, other_text) = (text(remote), text(&other));
    let clone = ["clone", "--branch", BRANCH, &remote_text, &other_text];
    run_git(f.root.path(), &clone);
    other
}

#[test]
fn commits_every_change_new_files_included_and_gives_the_hash_and_the_subject() {
    let f = fixture();
    add_worktree(&f);
    write(&f.worktree, "new.txt", "new\n");

    let result = commit_all(&f.worktree, "Add a file\n\nWith a body.").unwrap();

    assert_eq!(result.subject, "Add a file");
    assert_eq!(result.hash, rev(&f.worktree, "HEAD"));
    assert_eq!(status(&f.worktree), "");
    let message = run_git(&f.worktree, &["log", "-1", "--format=%B"]);
    assert_eq!(message.trim(), "Add a file\n\nWith a body.");
}

#[test]
fn refuses_when_there_is_nothing_to_commit() {
    let f = fixture();
    add_worktree(&f);
    assert_eq!(
        commit_all(&f.worktree, "Nothing"),
        Err(CommitError::NothingToCommit)
    );
    assert_eq!(
        CommitError::NothingToCommit.to_string(),
        "There is nothing to commit. The workspace matches its last commit."
    );
}

#[test]
fn takes_a_message_that_starts_with_a_dash_as_the_message() {
    let f = fixture();
    add_worktree(&f);
    write(&f.worktree, "a.txt", "a\n");

    assert_eq!(
        commit_all(&f.worktree, "--amend").unwrap().subject,
        "--amend"
    );
}

#[test]
fn pushes_the_branch_to_origin_and_sets_its_upstream() {
    let f = fixture();
    let remote = bare_origin(&f);
    add_worktree(&f);
    run_git(&f.worktree, &["commit", "--allow-empty", "-m", "work"]);

    push_branch(&f.worktree, BRANCH).unwrap();

    assert_eq!(rev(&remote, BRANCH), rev(&f.worktree, "HEAD"));
    let upstream = run_git(&f.worktree, &["rev-parse", "--abbrev-ref", "@{upstream}"]);
    assert_eq!(upstream.trim(), format!("origin/{BRANCH}"));
}

#[test]
fn fails_with_a_git_error_without_an_origin() {
    let f = fixture();
    add_worktree(&f);
    let error = push_branch(&f.worktree, BRANCH).unwrap_err();
    assert!(error.0.starts_with("git push failed: "), "{error}");
}

#[test]
fn skips_a_branch_with_no_upstream() {
    let f = fixture();
    add_worktree(&f);
    assert_eq!(pull_fast_forward(&f.worktree), Ok(false));
}

#[test]
fn brings_in_the_commits_that_were_merged_on_the_remote() {
    let f = fixture();
    let remote = bare_origin(&f);
    add_worktree(&f);
    push_branch(&f.worktree, BRANCH).unwrap();
    let other = clone_branch(&f, &remote);
    run_git(&other, &["commit", "--allow-empty", "-m", "merged subtask"]);
    run_git(&other, &["push", "origin", BRANCH]);

    assert_eq!(pull_fast_forward(&f.worktree), Ok(true));
    let subject = run_git(&f.worktree, &["log", "-1", "--format=%s"]);
    assert_eq!(subject.trim(), "merged subtask");
}

#[test]
fn fails_with_a_git_error_when_the_branch_went_another_way() {
    let f = fixture();
    let remote = bare_origin(&f);
    add_worktree(&f);
    push_branch(&f.worktree, BRANCH).unwrap();
    let other = clone_branch(&f, &remote);
    run_git(&other, &["commit", "--allow-empty", "-m", "merged subtask"]);
    run_git(&other, &["push", "origin", BRANCH]);
    run_git(
        &f.worktree,
        &["commit", "--allow-empty", "-m", "local only"],
    );

    let error = pull_fast_forward(&f.worktree).unwrap_err();
    assert!(error.0.starts_with("git pull failed: "), "{error}");
}

// A worktree whose `.gitignore` is committed, so `secret.txt` is ignored.
fn ignoring_worktree(f: &Fixture, ignored: &str) {
    add_worktree(f);
    write(&f.worktree, ".gitignore", &format!("{ignored}\n"));
    run_git(&f.worktree, &["add", ".gitignore"]);
    run_git(&f.worktree, &["commit", "-m", "ignore"]);
}

#[test]
fn commits_the_worktree_as_it_is_without_touching_its_branch_index_or_files() {
    let f = fixture();
    ignoring_worktree(&f, "secret.txt");
    write(&f.worktree, "a.ts", "export {}\n");
    write(&f.worktree, "secret.txt", "no\n");
    let head = rev(&f.worktree, "HEAD");
    let before = status(&f.worktree);

    let sha = snapshot_commit(&f.worktree, "Natamux CI check").unwrap();

    assert_eq!(rev(&f.worktree, &format!("{sha}^")), head);
    let listed = run_git(&f.worktree, &["ls-tree", "--name-only", &sha]);
    let files: Vec<&str> = listed.split('\n').collect();
    assert!(files.contains(&"a.ts"));
    assert!(!files.contains(&"secret.txt"));
    assert_eq!(rev(&f.worktree, "HEAD"), head);
    assert_eq!(status(&f.worktree), before);
}

#[test]
fn the_temp_index_lives_in_the_git_dir_and_is_removed() {
    let f = fixture();
    add_worktree(&f);
    let git_dir = PathBuf::from(run_git(&f.worktree, &["rev-parse", "--absolute-git-dir"]).trim());
    let index = TempIndex::new(&f.worktree).unwrap();
    assert_eq!(index.0.parent(), Some(git_dir.as_path()));
    drop(index);
    write(&f.worktree, "a.ts", "export {}\n");
    snapshot_commit(&f.worktree, "x").unwrap();
    read_commit_diff(&f.worktree, 10_000).unwrap();
    let left = std::fs::read_dir(&git_dir)
        .unwrap()
        .filter(|entry| {
            let name = entry.as_ref().unwrap().file_name();
            name.to_string_lossy().starts_with("natamux-index-")
        })
        .count();
    assert_eq!(left, 0);
}

#[test]
fn gives_head_itself_when_the_worktree_has_no_change_ignored_files_aside() {
    let f = fixture();
    ignoring_worktree(&f, "secret.txt");
    write(&f.worktree, "secret.txt", "no\n");
    let head = rev(&f.worktree, "HEAD");

    assert_eq!(snapshot_commit(&f.worktree, "Natamux CI check"), Ok(head));
}

#[test]
fn pushes_it_to_a_branch_of_origin_replaces_it_and_deletes_it() {
    let f = fixture();
    let remote = bare_origin(&f);
    add_worktree(&f);
    let first = snapshot_commit(&f.worktree, "one").unwrap();
    write(&f.worktree, "b.ts", "export {}\n");
    let second = snapshot_commit(&f.worktree, "two").unwrap();
    let ci_branch = format!("natamux-ci/{BRANCH}");

    push_ref(&f.worktree, &first, &ci_branch).unwrap();
    push_ref(&f.worktree, &second, &ci_branch).unwrap();
    assert_eq!(rev(&remote, &ci_branch), second);

    delete_remote_ref(&f.worktree, &ci_branch).unwrap();
    assert!(!git_succeeds(
        &remote,
        &["rev-parse", "--verify", &ci_branch]
    ));
}

#[test]
fn shows_every_change_commit_all_would_commit_without_touching_the_worktree() {
    let f = fixture();
    add_worktree(&f);
    write(&f.worktree, ".gitignore", "secret.txt\n");
    write(&f.worktree, "old.ts", "export const a = 1\n");
    run_git(&f.worktree, &["add", "."]);
    run_git(&f.worktree, &["commit", "-m", "start"]);
    write(&f.worktree, "old.ts", "export const a = 2\n");
    write(&f.worktree, "new.ts", "export const b = 1\n");
    write(&f.worktree, "secret.txt", "no\n");
    run_git(&f.worktree, &["add", "old.ts"]);
    let before = status(&f.worktree);

    let diff = read_commit_diff(&f.worktree, 60_000).unwrap();

    assert!(diff.contains("old.ts | 2 +-"), "{diff}");
    assert!(diff.contains("+export const a = 2"));
    assert!(diff.contains("+export const b = 1"));
    assert!(!diff.contains("secret.txt"));
    assert_eq!(status(&f.worktree), before);
}

#[test]
fn works_in_a_repository_without_a_commit() {
    let f = fixture();
    let empty = init_empty(&f);
    write(&empty, "a.ts", "export {}\n");

    assert!(
        read_commit_diff(&empty, 60_000)
            .unwrap()
            .contains("+export {}")
    );
}

// The number of the `[diff cut: N more characters]` line that ends a cut diff.
fn cut_count(text: &str) -> Option<usize> {
    let mark = "\n[diff cut: ";
    let at = text.rfind(mark)?;
    text[at + mark.len()..]
        .strip_suffix(" more characters]")?
        .parse()
        .ok()
}

#[test]
fn cuts_a_long_diff_and_keeps_the_stat() {
    let f = fixture();
    add_worktree(&f);
    write(&f.worktree, "big.txt", &"line\n".repeat(2000));

    let diff = read_commit_diff(&f.worktree, 500).unwrap();

    assert!(diff.contains("big.txt | 2000 +"));
    assert!(cut_count(&diff).is_some(), "{diff}");
    assert!(utf16_len(&diff) < 600);
}

#[test]
fn throws_nothing_to_commit_when_nothing_changed() {
    let f = fixture();
    add_worktree(&f);
    assert_eq!(
        read_commit_diff(&f.worktree, 60_000),
        Err(CommitError::NothingToCommit)
    );
}

#[test]
fn throws_nothing_to_commit_when_only_ignored_files_changed() {
    let f = fixture();
    ignoring_worktree(&f, "build/");
    std::fs::create_dir(f.worktree.join("build")).unwrap();
    write(&f.worktree.join("build"), "out.js", "x\n");

    assert_eq!(
        read_commit_diff(&f.worktree, 60_000),
        Err(CommitError::NothingToCommit)
    );
}

#[test]
fn throws_nothing_to_commit_in_a_new_repository_without_files() {
    let f = fixture();
    let empty = init_empty(&f);
    assert_eq!(
        read_commit_diff(&empty, 60_000),
        Err(CommitError::NothingToCommit)
    );
}

#[test]
fn counts_an_empty_new_file_as_a_change() {
    let f = fixture();
    add_worktree(&f);
    write(&f.worktree, "empty.txt", "");
    assert!(
        read_commit_diff(&f.worktree, 60_000)
            .unwrap()
            .contains("empty.txt")
    );
}

#[test]
fn shows_deleted_renamed_binary_staged_and_unicode_named_files() {
    let f = fixture();
    add_worktree(&f);
    let wt = &f.worktree;
    write(wt, "gone.ts", "export const gone = 1\n");
    write(wt, "old name.ts", "export const kept = \"a long line\"\n");
    run_git(wt, &["add", "."]);
    run_git(wt, &["commit", "-m", "start"]);
    std::fs::remove_file(wt.join("gone.ts")).unwrap();
    run_git(wt, &["mv", "old name.ts", "new name.ts"]);
    std::fs::write(wt.join("logo.png"), [0x89u8, 0x50, 0, 1, 2, 0, 3]).unwrap();
    write(wt, "tiếng việt.md", "xin chào\n");
    write(wt, "staged.ts", "export {}\n");
    run_git(wt, &["add", "staged.ts"]);

    let diff = read_commit_diff(wt, 60_000).unwrap();

    assert!(diff.contains("-export const gone = 1"));
    assert!(diff.contains("rename from old name.ts"));
    assert!(diff.contains("rename to new name.ts"));
    assert!(
        diff.lines()
            .any(|line| line.starts_with("Binary files ") && line.contains("logo.png differ")),
        "{diff}"
    );
    assert!(diff.contains("tiếng việt.md"));
    assert!(diff.contains("+xin chào"));
    assert!(diff.contains("staged.ts"));
}

#[test]
fn leaves_the_index_head_and_the_files_as_they_were_also_for_two_reads_at_once() {
    let f = fixture();
    add_worktree(&f);
    let wt = &f.worktree;
    write(wt, "staged.ts", "export {}\n");
    run_git(wt, &["add", "staged.ts"]);
    write(wt, "loose.ts", "export {}\n");
    let index_path = run_git(wt, &["rev-parse", "--git-path", "index"]);
    let index_file = wt.join(index_path.trim());
    let (status_before, head_before) = (status(wt), run_git(wt, &["rev-parse", "HEAD"]));
    let index_before = std::fs::read(&index_file).unwrap();

    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| read_commit_diff(wt, 60_000));
        let second = scope.spawn(|| read_commit_diff(wt, 60_000));
        (first.join().unwrap(), second.join().unwrap())
    });
    let (first, second) = (first.unwrap(), second.unwrap());

    assert_eq!(second, first);
    assert!(first.contains("loose.ts"));
    assert_eq!(status(wt), status_before);
    assert_eq!(run_git(wt, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(std::fs::read(&index_file).unwrap(), index_before);
}

// The whole diff of a worktree with one file, `big.txt`, that holds `content`.
fn full_diff_of(f: &Fixture, content: &str) -> String {
    add_worktree(f);
    write(&f.worktree, "big.txt", content);
    read_commit_diff(&f.worktree, usize::MAX).unwrap()
}

#[test]
fn counts_the_characters_it_cuts_exactly() {
    let f = fixture();
    let full = full_diff_of(&f, &"line\n".repeat(5000));

    let cut = read_commit_diff(&f.worktree, 2000).unwrap();

    let kept = &cut[..cut.rfind("\n[diff cut: ").unwrap()];
    let count = cut_count(&cut).unwrap();
    assert_eq!(utf16_len(kept), 2000);
    assert_eq!(utf16_len(kept) - 1 + count, utf16_len(&full) - 1);
    assert!(full.starts_with(kept));
}

#[test]
fn keeps_a_diff_that_fits_exactly_and_cuts_one_character_more() {
    let f = fixture();
    let full = full_diff_of(&f, &"line\n".repeat(100));
    let size = utf16_len(&full);

    assert_eq!(read_commit_diff(&f.worktree, size).unwrap(), full);
    let cut = read_commit_diff(&f.worktree, size - 1).unwrap();
    assert!(cut.ends_with("\n[diff cut: 1 more characters]"), "{cut}");
}

#[test]
fn keeps_the_whole_stat_when_the_stat_alone_is_over_the_cap() {
    let f = fixture();
    let full = full_diff_of(&f, &"line\n".repeat(100));

    let cut = read_commit_diff(&f.worktree, 10).unwrap();

    let stat = &full[..full.find("diff --git").unwrap()];
    assert!(cut.starts_with(stat));
    assert!(cut_count(&cut).is_some(), "{cut}");
}

#[test]
fn does_not_cut_an_emoji_in_half() {
    let f = fixture();
    let full = full_diff_of(&f, &"😀".repeat(200));
    let byte = full.find('😀').unwrap();
    let at = utf16_len(&full[..byte]);

    // Cut between the two halves of the first emoji.
    let cut = read_commit_diff(&f.worktree, at + 1).unwrap();

    assert_eq!(&cut[..cut.rfind("\n[diff cut: ").unwrap()], &full[..byte]);
}
