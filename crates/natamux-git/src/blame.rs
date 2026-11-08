//! Who last changed each line of a file: `git blame --porcelain`, read into runs of lines.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use natamux_proto::workspaces::changes::{BlameAnswer, BlameCommit, BlameRun};

use crate::exec::{GitError, git};

/// Blame of a big file with a long history can take a while; it runs only on a click.
const BLAME_TIMEOUT: Duration = Duration::from_secs(30);

/// The blame of `path` in the working tree, or at the last commit when `at_head`. A file git does
/// not track answers `tracked: false` and no runs.
pub fn read_blame(worktree: &Path, path: &str, at_head: bool) -> Result<BlameAnswer, GitError> {
    let mut args = vec!["blame", "--porcelain"];
    if at_head {
        args.push("HEAD");
    }
    args.extend(["--", path]);
    match git(worktree, &args, Some(BLAME_TIMEOUT), &[]) {
        Ok(text) => {
            let (commits, runs) = parse_porcelain(&text);
            Ok(BlameAnswer {
                path: path.to_string(),
                tracked: true,
                commits,
                runs,
            })
        }
        Err(error) if error.0.contains("no such path") => Ok(BlameAnswer {
            path: path.to_string(),
            tracked: false,
            ..BlameAnswer::default()
        }),
        Err(error) => Err(error),
    }
}

/// The commits in order of first use, and the runs of lines in file order. Each entry is a header
/// (`<sha> <old line> <new line> [<count>]`), the commit's fields the first time it shows, then
/// the line's text after a tab.
pub fn parse_porcelain(text: &str) -> (Vec<BlameCommit>, Vec<BlameRun>) {
    let mut commits: Vec<BlameCommit> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    // (line, commit index); line is 1-based as git gives it.
    let mut lines: Vec<(u32, usize)> = Vec::new();
    let mut current: Option<(u32, usize)> = None;
    for line in text.lines() {
        if line.starts_with('\t') {
            if let Some(found) = current.take() {
                lines.push(found);
            }
            continue;
        }
        if current.is_none() {
            let mut parts = line.split(' ');
            let (Some(sha), _, Some(new_line)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let Ok(new_line) = new_line.parse::<u32>() else {
                continue;
            };
            let index = *index_of.entry(sha.to_string()).or_insert_with(|| {
                commits.push(BlameCommit {
                    sha: sha.to_string(),
                    ..BlameCommit::default()
                });
                commits.len() - 1
            });
            current = Some((new_line, index));
            continue;
        }
        let Some((_, index)) = current else {
            continue;
        };
        let commit = &mut commits[index];
        if let Some(author) = line.strip_prefix("author ") {
            commit.author = author.to_string();
        } else if let Some(time) = line.strip_prefix("author-time ") {
            commit.time = time.trim().parse().unwrap_or(0);
        } else if let Some(summary) = line.strip_prefix("summary ") {
            commit.summary = summary.to_string();
        }
    }
    lines.sort_by_key(|(line, _)| *line);
    let mut runs: Vec<BlameRun> = Vec::new();
    for (line, commit) in lines {
        let start = line.saturating_sub(1);
        match runs.last_mut() {
            Some(run) if run.commit == commit && run.start + run.count == start => run.count += 1,
            _ => runs.push(BlameRun {
                start,
                count: 1,
                commit,
            }),
        }
    }
    (commits, runs)
}

#[cfg(test)]
#[path = "blame_tests.rs"]
mod tests;
