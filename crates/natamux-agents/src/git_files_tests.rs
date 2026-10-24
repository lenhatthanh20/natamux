//! The files give what git itself prints, for a checkout and a linked worktree.

use std::process::Command;

use super::*;
use crate::test_support::Fixture;

fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

#[test]
fn reads_the_head_and_the_common_folder_as_git_prints_them() {
    let f = Fixture::new();
    let repo = f.repo.clone();
    if git(&repo, &["init", "-q", "-b", "main"]).is_none() {
        return; // No git here; CI has it.
    }
    // No commit yet: git has no HEAD either.
    assert_eq!(head(&repo), None);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "one"]).unwrap();
    assert_eq!(head(&repo), git(&repo, &["rev-parse", "HEAD"]));
    let common = |cwd: &Path| {
        let printed = git(cwd, &["rev-parse", "--git-common-dir"]).unwrap();
        fs::canonicalize(cwd.join(printed)).unwrap()
    };
    assert_eq!(
        fs::canonicalize(common_dir(&repo).unwrap()).unwrap(),
        common(&repo)
    );

    let tree = f.home.join("tree");
    let tree_text = tree.to_string_lossy().into_owned();
    git(&repo, &["worktree", "add", "-q", "-b", "work", &tree_text]).unwrap();
    git(&tree, &["commit", "-q", "--allow-empty", "-m", "two"]).unwrap();
    assert_eq!(head(&tree), git(&tree, &["rev-parse", "HEAD"]));
    assert_ne!(head(&tree), head(&repo));
    assert_eq!(
        fs::canonicalize(common_dir(&tree).unwrap()).unwrap(),
        common(&tree)
    );

    // A packed branch is read from packed-refs.
    git(&repo, &["pack-refs", "--all"]).unwrap();
    assert_eq!(head(&tree), git(&tree, &["rev-parse", "HEAD"]));
}

#[test]
fn knows_nothing_of_a_folder_that_is_no_repository() {
    let f = Fixture::new();
    assert_eq!(head(&f.repo), None);
    assert_eq!(common_dir(&f.repo), None);
}
