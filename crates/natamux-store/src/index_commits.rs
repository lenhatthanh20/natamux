//! Group commit for the index. Writers that reach the index while a commit runs queue their rows;
//! when it ends, one of them commits all queued rows in one transaction and wakes the others. No
//! timer and no thread of its own: a lone writer commits at once.

use std::collections::HashMap;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use crate::task_index::{ChangeRows, IndexResult, TaskIndex};

#[derive(Default)]
struct CommitState {
    next: u64,
    queue: Vec<(u64, ChangeRows)>,
    running: bool,
    results: HashMap<u64, IndexResult<()>>,
}

#[derive(Default)]
pub struct Commits {
    state: Mutex<CommitState>,
    done: Condvar,
}

impl Commits {
    fn state(&self) -> MutexGuard<'_, CommitState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wait<'a>(&self, state: MutexGuard<'a, CommitState>) -> MutexGuard<'a, CommitState> {
        let woken = self.done.wait(state);
        woken.unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns once the transaction that holds `rows` has committed, or failed.
    pub fn commit(&self, index: &Mutex<TaskIndex>, rows: ChangeRows) -> IndexResult<()> {
        let mut state = self.state();
        let id = state.next;
        state.next += 1;
        state.queue.push((id, rows));
        loop {
            if let Some(result) = state.results.remove(&id) {
                return result;
            }
            if state.running {
                state = self.wait(state);
                continue;
            }
            state.running = true;
            let batch = std::mem::take(&mut state.queue);
            drop(state);
            let mut leading = Leading::new(self, &batch);
            leading.results = commit_batch(index, batch);
            drop(leading);
            state = self.state();
        }
    }
}

/// The commit of a batch. When it ends, also by a panic, its writers get their results and the
/// next batch may start; else they would wait for ever, each holding the turn of its task.
struct Leading<'a> {
    commits: &'a Commits,
    results: Vec<(u64, IndexResult<()>)>,
}

impl<'a> Leading<'a> {
    fn new(commits: &'a Commits, batch: &[(u64, ChangeRows)]) -> Leading<'a> {
        let failed = || Err("The index commit stopped".to_string());
        let results = batch.iter().map(|(id, _)| (*id, failed())).collect();
        Leading { commits, results }
    }
}

impl Drop for Leading<'_> {
    fn drop(&mut self) {
        let mut state = self.commits.state();
        state.results.extend(self.results.drain(..));
        state.running = false;
        drop(state);
        self.commits.done.notify_all();
    }
}

/// One transaction for the whole batch. When it fails nothing of it is kept, so each change is
/// tried again alone: a bad change fails only its own writer.
fn commit_batch(
    index: &Mutex<TaskIndex>,
    batch: Vec<(u64, ChangeRows)>,
) -> Vec<(u64, IndexResult<()>)> {
    let mut index = index.lock().unwrap_or_else(PoisonError::into_inner);
    let (ids, rows): (Vec<u64>, Vec<ChangeRows>) = batch.into_iter().unzip();
    let all = index.record_rows(&rows);
    if all.is_ok() || rows.len() == 1 {
        return ids.into_iter().map(|id| (id, all.clone())).collect();
    }
    ids.into_iter()
        .zip(&rows)
        .map(|(id, change)| (id, index.record_rows(std::slice::from_ref(change))))
        .collect()
}

#[cfg(test)]
#[path = "index_commits_tests.rs"]
mod tests;
