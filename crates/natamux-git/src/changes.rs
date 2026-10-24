//! Ports `packages/daemon/src/git/changes.ts`: the files of a worktree that differ from its last
//! commit, with their line counts, and the size of a diff.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use natamux_core::js::utf16_len;
use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::{ChangeEntry, DiffStat};
use serde_json::{Value, json};

use crate::exec::{GitError, git, with_git_config};
use crate::file_diff::{MAX_FILE_BYTES, looks_binary, read_inside};

/// Added and removed lines of one file. None for a file that is binary or not known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineCounts {
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

pub fn status_from_codes(index: char, worktree: char) -> ChangeStatus {
    let conflict = index == 'U'
        || worktree == 'U'
        || (index == 'A' && worktree == 'A')
        || (index == 'D' && worktree == 'D');
    if conflict {
        return ChangeStatus::Conflict;
    }
    if index == '?' && worktree == '?' {
        return ChangeStatus::Untracked;
    }
    if index == 'R' || worktree == 'R' {
        return ChangeStatus::Renamed;
    }
    if matches!(index, 'A' | 'C') || matches!(worktree, 'A' | 'C') {
        return ChangeStatus::Added;
    }
    if index == 'D' || worktree == 'D' {
        return ChangeStatus::Deleted;
    }
    ChangeStatus::Modified
}

fn entry(path: &str, status: ChangeStatus, original_path: Option<String>) -> ChangeEntry {
    ChangeEntry {
        path: path.to_string(),
        status,
        original_path,
        added: None,
        removed: None,
    }
}

/// Reads the output of `git status --porcelain=v1 -z`. Each entry is "XY path" and a NUL; a renamed
/// or copied file is followed by one more NUL-ended item, its old path.
pub fn parse_status(output: &str) -> Vec<ChangeEntry> {
    let parts: Vec<&str> = output.split('\0').collect();
    let mut entries = Vec::new();
    let mut position = 0;
    while position < parts.len() {
        let item = parts[position];
        position += 1;
        if item.is_empty() || utf16_len(item) < 4 {
            continue;
        }
        let mut codes = item.chars();
        let index = codes.next().unwrap_or(' ');
        let worktree = codes.next().unwrap_or(' ');
        let moved = matches!(index, 'R' | 'C') || matches!(worktree, 'R' | 'C');
        let mut original_path = None;
        if moved {
            original_path = parts.get(position).map(|path| path.to_string());
            position += 1;
        }
        // The first three characters are the two codes and a space.
        let path = item.char_indices().nth(3).map_or("", |(at, _)| &item[at..]);
        entries.push(entry(
            path,
            status_from_codes(index, worktree),
            original_path,
        ));
    }
    entries
}

/// Reads `git diff --numstat -z`. A record is "added<TAB>removed<TAB>path" and a NUL. For a
/// rename the path is empty and two NUL-ended items follow, the old path and the new one. Binary
/// files have "-" for both counts. Keyed by the path the file has now.
pub fn parse_numstat_z(output: &str) -> HashMap<String, LineCounts> {
    let parts: Vec<&str> = output.split('\0').collect();
    let mut counts = HashMap::new();
    let mut position = 0;
    while position < parts.len() {
        let item = parts[position];
        position += 1;
        let Some((added, rest)) = item.split_once('\t') else {
            continue;
        };
        let Some((removed, path)) = rest.split_once('\t') else {
            continue;
        };
        let mut path = path;
        if path.is_empty() {
            // The old path is at `position`, the new one after it.
            path = parts.get(position + 1).copied().unwrap_or("");
            position += 2;
        }
        if path.is_empty() {
            continue;
        }
        let number = |text: &str| if text == "-" { None } else { text.parse().ok() };
        counts.insert(
            path.to_string(),
            LineCounts {
                added: number(added),
                removed: number(removed),
            },
        );
    }
    counts
}

/// The lines of a text file: one for each line break, and one more for a last line without one.
pub fn count_text_lines(bytes: &[u8]) -> u64 {
    let breaks = bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
    match bytes.last() {
        Some(last) if *last != b'\n' => breaks + 1,
        _ => breaks,
    }
}

// Reading every new file on each refresh would be slow, so only this many are counted.
const MAX_COUNTED_UNTRACKED: usize = 300;
const MAX_CACHED_COUNTS: usize = 5000;
// Node counts all of them at once; a bound keeps the number of threads sane.
const COUNT_THREADS: usize = 8;

struct CachedLines {
    size: u64,
    mtime: Option<SystemTime>,
    lines: Option<u64>,
}

// The line count of an untracked file, by its real path. It is used while the size and the time
// of the last change are the same.
static UNTRACKED_LINES: LazyLock<Mutex<HashMap<PathBuf, CachedLines>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// A cache that a panicking thread left is still a cache.
fn lock_cache() -> std::sync::MutexGuard<'static, HashMap<PathBuf, CachedLines>> {
    UNTRACKED_LINES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn untracked_counts(worktree: &Path, path: &str) -> LineCounts {
    let none = LineCounts::default();
    let Ok(Some(file)) = read_inside(worktree, path) else {
        return none;
    };
    // Git lists a nested repository as one untracked folder.
    if !file.is_file || file.size > MAX_FILE_BYTES {
        return none;
    }
    let hit = lock_cache()
        .get(&file.real)
        .filter(|cached| cached.size == file.size && cached.mtime == file.mtime)
        .map(|cached| cached.lines);
    let lines = match hit {
        Some(lines) => lines,
        None => {
            // An agent can delete the file after the stat. One file must not fail the whole list.
            let Ok(bytes) = file.read() else {
                return none;
            };
            let lines = (!looks_binary(&bytes)).then(|| count_text_lines(&bytes));
            let mut cache = lock_cache();
            if cache.len() >= MAX_CACHED_COUNTS {
                cache.clear();
            }
            let cached = CachedLines {
                size: file.size,
                mtime: file.mtime,
                lines,
            };
            cache.insert(file.real.clone(), cached);
            lines
        }
    };
    match lines {
        Some(lines) => LineCounts {
            added: Some(lines),
            removed: Some(0),
        },
        None => none,
    }
}

/// The settings of the status and diff reads the dashboard polls. If they took index.lock, an
/// agent's git add or commit in the same worktree would fail. `git diff` writes the index even
/// with optional locks off, so `diff.autoRefreshIndex` is off too.
pub fn read_env() -> Vec<(String, String)> {
    let count = std::env::var("GIT_CONFIG_COUNT").ok();
    let mut env = with_git_config(count.as_deref(), &[("diff.autoRefreshIndex", "false")]);
    env.push(("GIT_OPTIONAL_LOCKS".to_string(), "0".to_string()));
    env
}

// The counts of the untracked files at `paths`, on at most `COUNT_THREADS` threads.
fn count_untracked(worktree: &Path, paths: &[&str]) -> Vec<LineCounts> {
    let next = &AtomicUsize::new(0);
    let threads = COUNT_THREADS.min(paths.len());
    let mut counted = vec![LineCounts::default(); paths.len()];
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(move || {
                    let mut mine = Vec::new();
                    loop {
                        let at = next.fetch_add(1, AtomicOrdering::Relaxed);
                        let Some(path) = paths.get(at) else {
                            return mine;
                        };
                        mine.push((at, untracked_counts(worktree, path)));
                    }
                })
            })
            .collect();
        for worker in workers {
            // A worker that panicked leaves its files without counts.
            for (at, counts) in worker.join().unwrap_or_default() {
                counted[at] = counts;
            }
        }
    });
    counted
}

fn line_counts(worktree: &Path, entries: Vec<ChangeEntry>) -> Vec<ChangeEntry> {
    // `-M` finds renames even when the user's git config turns them off. A repository without a
    // commit has no HEAD to compare with: no counts then.
    let tracked = git(
        worktree,
        &["diff", "--numstat", "-z", "-M", "HEAD", "--"],
        None,
        &read_env(),
    )
    .map(|output| parse_numstat_z(&output))
    .unwrap_or_default();
    let untracked: Vec<&str> = entries
        .iter()
        .filter(|entry| entry.status == ChangeStatus::Untracked)
        .map(|entry| entry.path.as_str())
        .take(MAX_COUNTED_UNTRACKED)
        .collect();
    let counted = count_untracked(worktree, &untracked);
    let mut untracked_seen = 0;
    let mut result = Vec::with_capacity(entries.len());
    for mut entry in entries {
        let counts = if entry.status == ChangeStatus::Untracked {
            untracked_seen += 1;
            counted.get(untracked_seen - 1).copied().unwrap_or_default()
        } else {
            tracked.get(&entry.path).copied().unwrap_or_default()
        };
        entry.added = counts.added;
        entry.removed = counts.removed;
        result.push(entry);
    }
    result
}

// `localeCompare` for file paths, close to ICU's root collation: whitespace and punctuation, then
// digits, then letters without regard to case; lower case first on a tie.
fn locale_key(c: char) -> (u32, u32) {
    const PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";
    if let Some(place) = PUNCTUATION.find(c) {
        return (1, place as u32);
    }
    if c.is_whitespace() {
        return (0, c as u32);
    }
    if c.is_ascii_digit() {
        return (2, c as u32);
    }
    (3, c.to_lowercase().next().unwrap_or(c) as u32)
}

/// Node's `a.localeCompare(b)`, for names of files and branches. Not the whole of ICU: accents
/// and other scripts compare by code point.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let primary = a.chars().map(locale_key).cmp(b.chars().map(locale_key));
    primary.then_with(|| {
        a.chars()
            .zip(b.chars())
            .map(|(x, y)| x.is_uppercase().cmp(&y.is_uppercase()))
            .find(|order| order.is_ne())
            .unwrap_or(Ordering::Equal)
    })
}

/// The files of a worktree that differ from its last commit, including files not tracked yet,
/// sorted by path. With `counts`, each file also has the lines it adds and removes (None when not
/// known). Without, it costs one git call and the counts are None: Node leaves the keys out, which
/// `change_json` does with `with_counts` false.
pub fn list_changes(worktree: &Path, counts: bool) -> Result<Vec<ChangeEntry>, GitError> {
    let output = git(
        worktree,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        None,
        &read_env(),
    )?;
    let mut entries = parse_status(&output);
    entries.sort_by(|a, b| locale_compare(&a.path, &b.path));
    Ok(if counts {
        line_counts(worktree, entries)
    } else {
        entries
    })
}

/// A change as Node sends it: `path`, `status`, `originalPath`, and with `with_counts` also
/// `added` and `removed` (null when not known).
pub fn change_json(entry: &ChangeEntry, with_counts: bool) -> Value {
    let mut value = json!({
        "path": entry.path,
        "status": entry.status.letter(),
        "originalPath": entry.original_path,
    });
    if with_counts && let Some(map) = value.as_object_mut() {
        map.insert("added".to_string(), json!(entry.added));
        map.insert("removed".to_string(), json!(entry.removed));
    }
    value
}

/// Sums `git diff --numstat`. Binary files count as files, without lines.
pub fn parse_numstat(output: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    for line in output.split('\n') {
        let mut fields = line.split('\t');
        let (Some(added), Some(removed)) = (fields.next(), fields.next()) else {
            continue;
        };
        stat.files += 1;
        stat.added += added.parse().unwrap_or(0);
        stat.removed += removed.parse().unwrap_or(0);
    }
    stat
}

/// The worktree against the commit where its branch left `base`, or against HEAD without a base.
pub fn read_diff_stat(worktree: &Path, base: Option<&str>) -> Result<DiffStat, GitError> {
    // Node runs `merge-base HEAD <base>`, then `diff --numstat <that> --`; `--merge-base` is the
    // same diff in one git process (diff stats are read often).
    let args: Vec<&str> = match base {
        None => vec!["diff", "--numstat", "HEAD", "--"],
        Some(base) => vec!["diff", "--numstat", "--merge-base", base, "--"],
    };
    let output = git(worktree, &args, None, &read_env())?;
    Ok(parse_numstat(&output))
}

#[cfg(test)]
#[path = "changes_tests.rs"]
mod tests;
