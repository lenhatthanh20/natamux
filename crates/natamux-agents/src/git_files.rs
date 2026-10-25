//! What `git rev-parse --git-common-dir` and `git rev-parse HEAD` print, read from the files of
//! the repository. A stage start asks both several times, and a git process costs a few
//! milliseconds each; the files cost none. Anything unexpected gives None, and the caller asks
//! git as Node does.

use std::fs;
use std::path::{Path, PathBuf};

/// The git folder of a worktree or checkout, and the folder its worktrees share.
fn git_dirs(worktree: &Path) -> Option<(PathBuf, PathBuf)> {
    let dot_git = worktree.join(".git");
    let meta = fs::metadata(&dot_git).ok()?;
    if meta.is_dir() {
        return Some((dot_git.clone(), dot_git));
    }
    // A linked worktree: `.git` is a file with `gitdir: <path>`, and that folder names the
    // shared one in `commondir`.
    let text = fs::read_to_string(&dot_git).ok()?;
    let target = text.trim_end().strip_prefix("gitdir: ")?;
    let git_dir = worktree.join(target);
    let common = match fs::read_to_string(git_dir.join("commondir")) {
        Ok(text) => git_dir.join(text.trim_end()),
        Err(_) => git_dir.clone(),
    };
    Some((git_dir, common))
}

/// The folder all worktrees of the repository share.
pub fn common_dir(worktree: &Path) -> Option<PathBuf> {
    git_dirs(worktree).map(|(_, common)| common)
}

fn is_sha(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The commit HEAD is on. None for a HEAD with no commit yet, or files this does not know.
pub fn head(worktree: &Path) -> Option<String> {
    let (git_dir, common) = git_dirs(worktree)?;
    let text = fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let text = text.trim_end();
    let Some(name) = text.strip_prefix("ref: ") else {
        return is_sha(text).then(|| text.to_string());
    };
    for folder in [&git_dir, &common] {
        if let Ok(sha) = fs::read_to_string(folder.join(name)) {
            let sha = sha.trim_end();
            return is_sha(sha).then(|| sha.to_string());
        }
    }
    let packed = fs::read_to_string(common.join("packed-refs")).ok()?;
    packed.lines().find_map(|line| {
        let (sha, reference) = line.split_once(' ')?;
        (reference == name && is_sha(sha)).then(|| sha.to_string())
    })
}

#[cfg(test)]
#[path = "git_files_tests.rs"]
mod tests;
