//! `workspace-changes.ts`: the files of a workspace: its changes, their diffs, its graph and the
//! files explorer. `WorkspaceChanges` holds the caches and the last rollback of each task; the
//! calls are in the `impl WorkspaceService` block below.
//!
//! A cached read is shared by every thread that asks while it runs (`TtlCache`); the threads wait
//! for it with `block_on`, the one that started it runs git.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use futures::executor::block_on;
use natamux_git::changes::list_changes;
use natamux_git::commit_push::snapshot_commit;
use natamux_git::exec::{GitError, current_branch};
use natamux_git::file_diff::{read_file_diff, read_file_image};
use natamux_git::graph::{DEFAULT_LIMIT, read_git_graph};
use natamux_git::rollback::{has_commit, restore_from_snapshot, rollback_files, rollback_paths};
use natamux_proto::workspaces::changes::{ChangeStatus, FileDiff, ImageSide};
use natamux_proto::workspaces::files::{SavedFile, TreeEntry, WorkspaceFile};
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph};

use crate::shared::jobs::Clock;
use crate::shared::ttl_cache::TtlCache;
use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};
use crate::workspaces::files::{list_folder, read_workspace_file, write_workspace_file};
use crate::workspaces::service::WorkspaceService;

/// Dashboards ask for the changes, the graph and the diff stats on timers. Within this time they
/// share one read, so git runs less often. A change made through this service is seen at once.
pub const GIT_READ_MS: i64 = 5000;

/// What the undo of a rollback needs: the snapshot made just before it, and the paths it touched.
struct LastRollback {
    snapshot: String,
    paths: Vec<String>,
    files: usize,
}

pub struct WorkspaceChanges {
    changes: TtlCache<Vec<ChangeEntry>, GitError>,
    graphs: TtlCache<GitGraph, GitError>,
    /// Only the last rollback of each task can be undone. A daemon restart forgets it.
    rollbacks: Mutex<HashMap<String, LastRollback>>,
}

impl WorkspaceChanges {
    pub fn new(now: Clock) -> WorkspaceChanges {
        WorkspaceChanges {
            changes: TtlCache::new(GIT_READ_MS, Arc::clone(&now)),
            graphs: TtlCache::new(GIT_READ_MS, now),
            rollbacks: Mutex::default(),
        }
    }

    /// The workspace of the task may have changed.
    pub fn forget(&self, task_id: &str) {
        self.changes.forget(Some(task_id));
        self.graphs.forget(Some(task_id));
    }

    pub fn drop_rollback(&self, task_id: &str) {
        self.rollbacks().remove(task_id);
    }

    fn rollbacks(&self) -> MutexGuard<'_, HashMap<String, LastRollback>> {
        self.rollbacks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl WorkspaceService {
    /// The files of the workspace that differ from its last commit, with the lines of each.
    /// `fresh` reads git now, and the answer replaces the kept one.
    pub fn changes(&self, task_id: &str, fresh: bool) -> WorkspaceResult<Vec<ChangeEntry>> {
        let workspace = self.workspace_of(task_id)?;
        if fresh {
            self.reads.changes.forget(Some(task_id));
        }
        let read =
            self.reads.changes.get(
                task_id,
                move || async move { list_changes(&workspace, true) },
            );
        Ok(block_on(read)?)
    }

    /// Only a file that is in the list of changes can be read: this is what keeps the request
    /// from reaching any other file.
    pub fn file_diff(&self, task_id: &str, path: &str) -> WorkspaceResult<FileDiff> {
        let workspace = self.workspace_of(task_id)?;
        let change = self.changed_file(task_id, path)?;
        Ok(read_file_diff(&workspace, &change)?)
    }

    /// One side of a changed image, with the same guard as `file_diff`: the MIME type and the
    /// bytes.
    pub fn file_image(
        &self,
        task_id: &str,
        path: &str,
        side: ImageSide,
    ) -> WorkspaceResult<(String, Vec<u8>)> {
        let workspace = self.workspace_of(task_id)?;
        let change = self.changed_file(task_id, path)?;
        Ok(read_file_image(&workspace, &change, side)?)
    }

    /// The files explorer: one folder, one file, and a save by hand.
    pub fn tree(&self, task_id: &str, path: &str) -> WorkspaceResult<Vec<TreeEntry>> {
        list_folder(&self.workspace_of(task_id)?, path)
    }

    pub fn read_file(&self, task_id: &str, path: &str) -> WorkspaceResult<WorkspaceFile> {
        read_workspace_file(&self.workspace_of(task_id)?, path)
    }

    pub fn write_file(
        &self,
        task_id: &str,
        path: &str,
        text: &str,
        version: &str,
    ) -> WorkspaceResult<SavedFile> {
        let saved = write_workspace_file(&self.workspace_of(task_id)?, path, text, version)?;
        self.touch(task_id);
        Ok(saved)
    }

    /// Puts changed files back as they are in the last commit; a new file is deleted. Only files
    /// in the list of changes can be named. A snapshot of the whole worktree comes first, for the
    /// undo. Answers how many files it rolled back.
    pub fn rollback(&self, task_id: &str, paths: &[String]) -> WorkspaceResult<usize> {
        self.exclusive(task_id, "Rolling back files", || {
            let workspace = self.workspace_of(task_id)?;
            if !has_commit(&workspace) {
                return Err(WorkspaceError::Conflict(
                    "There is no commit to roll back to.".into(),
                ));
            }
            let changes = self.changes(task_id, true)?;
            let entries = rollback_entries(&changes, paths)?;
            let snapshot = snapshot_commit(&workspace, "Natamux rollback")?;
            // Kept before the files change, so a rollback that stops halfway can be undone too.
            self.reads.rollbacks().insert(
                task_id.to_string(),
                LastRollback {
                    snapshot,
                    paths: rollback_paths(&entries),
                    files: entries.len(),
                },
            );
            let rolled = rollback_files(&workspace, &entries);
            self.touch(task_id);
            rolled?;
            Ok(entries.len())
        })
    }

    /// Brings back the files of the last rollback as they were just before it. Answers how many.
    pub fn undo_rollback(&self, task_id: &str) -> WorkspaceResult<usize> {
        self.exclusive(task_id, "Undoing rollback", || {
            let workspace = self.workspace_of(task_id)?;
            let last = self.reads.rollbacks().remove(task_id);
            let Some(last) = last else {
                return Err(WorkspaceError::Conflict("Nothing to undo.".into()));
            };
            let restored = restore_from_snapshot(&workspace, &last.snapshot, &last.paths);
            self.touch(task_id);
            restored?;
            Ok(last.files)
        })
    }

    /// The history of the task branch next to the branch that the main checkout is on. `fresh` as
    /// for `changes`.
    pub fn graph(&self, task_id: &str, fresh: bool) -> WorkspaceResult<GitGraph> {
        let workspace = self.workspace_of(task_id)?;
        let repo_dir = self.repo_dir_of(self.tasks.get_task(task_id)?.repo.as_deref())?;
        if fresh {
            self.reads.graphs.forget(Some(task_id));
        }
        let read = self.reads.graphs.get(task_id, move || async move {
            read_git_graph(
                &workspace,
                current_branch(&repo_dir).as_deref(),
                DEFAULT_LIMIT,
            )
        });
        Ok(block_on(read)?)
    }

    pub(crate) fn drop_rollback(&self, task_id: &str) {
        self.reads.drop_rollback(task_id);
    }

    // The kept list first: the dashboard asks for the changes and a diff at the same moment. A file
    // that changed after that list was read gets one fresh read.
    fn changed_file(&self, task_id: &str, path: &str) -> WorkspaceResult<ChangeEntry> {
        let find = |entries: Vec<ChangeEntry>| entries.into_iter().find(|entry| entry.path == path);
        if let Some(change) = find(self.changes(task_id, false)?) {
            return Ok(change);
        }
        find(self.changes(task_id, true)?)
            .ok_or_else(|| WorkspaceError::NotChanged(path.to_string()))
    }
}

// The entries to roll back, each path once, in the order given.
fn rollback_entries(
    changes: &[ChangeEntry],
    paths: &[String],
) -> WorkspaceResult<Vec<ChangeEntry>> {
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for path in paths {
        if !seen.insert(path) {
            continue;
        }
        let entry = changes
            .iter()
            .find(|entry| &entry.path == path)
            .ok_or_else(|| WorkspaceError::NotChanged(path.clone()))?;
        if entry.status == ChangeStatus::Conflict {
            return Err(WorkspaceError::Conflict(format!(
                "{path} has a conflict. Resolve it first"
            )));
        }
        entries.push(entry.clone());
    }
    Ok(entries)
}
