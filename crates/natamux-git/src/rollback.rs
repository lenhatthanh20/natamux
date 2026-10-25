//! Ports `packages/daemon/src/git/rollback.ts`: puts files back as they are in HEAD, and undoes
//! that from a snapshot.

use std::collections::HashSet;
use std::path::Path;

use natamux_core::js::utf16_len;
use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::ChangeEntry;

use crate::exec::{GitError, git};

/// Paths go on the command line, which the system limits (1 MiB on macOS, environment included).
const MAX_BATCH_CHARS: usize = 64 * 1024;

/// A file named like `a*.ts` or `:(top)x` names only itself, never a pattern of other files.
fn literal_env() -> Vec<(String, String)> {
    vec![("GIT_LITERAL_PATHSPECS".into(), "1".into())]
}

/// The paths in groups whose length stays under `MAX_BATCH_CHARS`. A group has one path at least.
pub fn path_batches(paths: &[String]) -> Vec<Vec<String>> {
    let mut batches = Vec::new();
    let mut batch: Vec<String> = Vec::new();
    let mut chars = 0;
    for path in paths {
        let length = utf16_len(path);
        if !batch.is_empty() && chars + length + 1 > MAX_BATCH_CHARS {
            batches.push(std::mem::take(&mut batch));
            chars = 0;
        }
        batch.push(path.clone());
        chars += length + 1;
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

fn each_batch(worktree: &Path, args: &[&str], paths: &[String]) -> Result<(), GitError> {
    let env = literal_env();
    for batch in path_batches(paths) {
        let mut full = args.to_vec();
        full.push("--");
        full.extend(batch.iter().map(String::as_str));
        git(worktree, &full, None, &env)?;
    }
    Ok(())
}

pub fn has_commit(worktree: &Path) -> bool {
    git(
        worktree,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
        None,
        &[],
    )
    .is_ok()
}

/// Every path a rollback of these files touches: a rename has its old path too.
pub fn rollback_paths(entries: &[ChangeEntry]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|entry| match (&entry.status, &entry.original_path) {
            (ChangeStatus::Renamed, Some(original)) => vec![original.clone(), entry.path.clone()],
            _ => vec![entry.path.clone()],
        })
        .collect()
}

/// Puts the files back as they are in HEAD, in the index and on disk. A file that HEAD does not
/// have is deleted. `git clean` deletes only files git does not track, never ignored ones.
pub fn rollback_files(worktree: &Path, entries: &[ChangeEntry]) -> Result<(), GitError> {
    let (untracked, tracked): (Vec<&ChangeEntry>, Vec<&ChangeEntry>) = entries
        .iter()
        .partition(|entry| entry.status == ChangeStatus::Untracked);
    let tracked: Vec<ChangeEntry> = tracked.into_iter().cloned().collect();
    each_batch(
        worktree,
        &["restore", "--source=HEAD", "--staged", "--worktree"],
        &rollback_paths(&tracked),
    )?;
    let untracked: Vec<String> = untracked.iter().map(|entry| entry.path.clone()).collect();
    each_batch(worktree, &["clean", "-f", "-q"], &untracked)
}

/// Undoes a rollback: each path gets its file from the snapshot made before it. A path that the
/// snapshot does not have (a deleted file, the old name of a renamed one) is deleted again. Only
/// the files come back, not the index: a commit adds every file anyway.
pub fn restore_from_snapshot(
    worktree: &Path,
    snapshot: &str,
    paths: &[String],
) -> Result<(), GitError> {
    let env = literal_env();
    let mut kept: HashSet<String> = HashSet::new();
    for batch in path_batches(paths) {
        let mut args = vec!["ls-tree", "-r", "-z", "--name-only", snapshot, "--"];
        args.extend(batch.iter().map(String::as_str));
        let out = git(worktree, &args, None, &env)?;
        kept.extend(
            out.split('\0')
                .filter(|path| !path.is_empty())
                .map(str::to_string),
        );
    }
    let (present, gone): (Vec<String>, Vec<String>) =
        paths.iter().cloned().partition(|path| kept.contains(path));
    let source = format!("--source={snapshot}");
    each_batch(worktree, &["restore", &source, "--worktree"], &present)?;
    each_batch(worktree, &["rm", "-q", "-f", "--ignore-unmatch"], &gone)
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
