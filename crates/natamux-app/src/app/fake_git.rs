//! The fake's git of screen 4: the files of a workspace with their two texts, the graph, and the
//! daemon's writes on them (commit, draft a message, rollback with its undo, comment for the
//! agent). Each write changes the fake data and sends `WorkspaceChanged`, as the daemon does.
//! The data is `fake_git.json`.

use std::collections::BTreeMap;

use natamux_client::{ClientError, DashboardEvent};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::changes::{
    COMMIT_SUBJECT_MAX, ChangeStatus, CommitAnswer, CommitMessageDraft, DraftSource, FileDiff,
    ImageSide, RollbackResult, image_type_of,
};
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph, GraphCommit, PushStatus};
use serde_json::{Value, json};

use super::{Answer, Data, FakeDaemon, NOW};

const SEED: &str = include_str!("fake_git.json");
/// The working-tree side of the seeded `public/banner.png`.
const BANNER: &[u8] = include_bytes!("../../assets/fake/banner.png");
const BANNER_PATH: &str = "public/banner.png";
/// The terminal the daemon pastes a comment into, and the commit author of the fake.
const MAIN_TERMINAL: &str = "1";
const AUTHOR: &str = "Natamux e2e";

/// The two texts of each changed file, and the one rollback that can be undone, per task.
#[derive(Default)]
pub(super) struct GitState {
    files: BTreeMap<String, Vec<FileDiff>>,
    undo: BTreeMap<String, Vec<ChangeEntry>>,
}

fn status(code: u16, message: String) -> ClientError {
    ClientError::Status {
        status: code,
        message,
    }
}

impl Data {
    /// Adds the changes, graph and file texts of screen 4 to the seed.
    pub(super) fn add_git_seed(&mut self) {
        let seed: Value = serde_json::from_str(SEED).unwrap_or(Value::Null);
        let changes: BTreeMap<String, Vec<ChangeEntry>> = part!(seed, "changes");
        self.changes.extend(changes);
        let graphs: BTreeMap<String, GitGraph> = part!(seed, "graphs");
        self.graphs.extend(graphs);
        self.git.files = part!(seed, "files");
    }

    fn change(&self, task_id: &str, path: &str) -> Answer<&ChangeEntry> {
        let list = self.changes.get(task_id);
        let found = list.and_then(|list| list.iter().find(|entry| entry.path == path));
        found.ok_or_else(|| status(404, format!("{path} is not changed in {task_id}")))
    }

    fn changed(&self, task_id: &str) -> &[ChangeEntry] {
        self.changes.get(task_id).map_or(&[], Vec::as_slice)
    }

    /// `StageRunner.agentRuns` for Terminal 1: an agent there that is not restoring or done.
    fn agent_runs(&self, task_id: &str) -> bool {
        let Some(list) = self.terminals.get(task_id) else {
            return false;
        };
        let done = list
            .lanes
            .get(MAIN_TERMINAL)
            .is_some_and(|lane| lane.state == Some(TaskState::Done));
        list.agents.iter().any(|name| name == MAIN_TERMINAL)
            && !list.restoring.contains_key(MAIN_TERMINAL)
            && !done
    }

    fn workspace_changed(&mut self, task_id: &str) {
        self.send(DashboardEvent::WorkspaceChanged {
            task_id: task_id.into(),
        });
    }

    /// Stores a person's event of the task and moves its `updatedAt`, as the daemon's records do.
    fn touch(&mut self, task_id: &str, kind: &str, payload: Value) -> Answer<Task> {
        self.record(task_id, "human", kind, payload);
        let task = self.task_mut(task_id)?;
        task.updated_at = NOW.into();
        let task = task.clone();
        self.tasks_changed();
        Ok(task)
    }
}

/// A new commit on top of the graph: the branch ref moves to it, and the counts follow.
fn add_commit(graph: &mut GitGraph, subject: String) -> String {
    let hash = format!("{:07x}", 0xc0de000 + graph.commits.len());
    let branch = graph.branch.clone();
    if let Some(branch) = &branch {
        for commit in &mut graph.commits {
            commit.refs.retain(|name| name != branch);
        }
    }
    graph.commits.insert(
        0,
        GraphCommit {
            hash: hash.clone(),
            parents: Some(graph.head.clone())
                .filter(|head| !head.is_empty())
                .into_iter()
                .collect(),
            author: AUTHOR.into(),
            date: NOW.into(),
            subject,
            refs: branch.into_iter().collect(),
            on_branch: true,
        },
    );
    graph.head = hash.clone();
    graph.ahead = Some(graph.ahead.unwrap_or(0) + 1);
    if let Some(PushStatus::New { unpushed } | PushStatus::Tracked { unpushed, .. }) =
        &mut graph.push
    {
        *unpushed += 1;
    }
    hash
}

impl FakeDaemon {
    pub fn file_diff(&self, task_id: &str, path: &str) -> Answer<FileDiff> {
        let data = self.data();
        let entry = data.change(task_id, path)?;
        let saved = data.git.files.get(task_id);
        let saved = saved.and_then(|files| files.iter().find(|file| file.path == path));
        let mut diff = saved.cloned().unwrap_or(FileDiff {
            path: path.into(),
            status: entry.status,
            original: None,
            modified: None,
            binary: false,
            too_large: false,
        });
        diff.status = entry.status;
        Ok(diff)
    }

    pub fn file_image(&self, task_id: &str, path: &str, side: ImageSide) -> Answer<Vec<u8>> {
        if image_type_of(path).is_none() {
            return Err(status(400, format!("{path} is not an image")));
        }
        let data = self.data();
        let gone = match side {
            ImageSide::Original => matches!(
                data.change(task_id, path)?.status,
                ChangeStatus::Added | ChangeStatus::Untracked
            ),
            ImageSide::Modified => data.change(task_id, path)?.status == ChangeStatus::Deleted,
        };
        if gone || path != BANNER_PATH {
            return Err(status(404, format!("{path} has no {} image", side.name())));
        }
        Ok(BANNER.to_vec())
    }

    pub fn commit(&self, task_id: &str, message: String) -> Answer<CommitAnswer> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        if data.changed(task_id).is_empty() {
            return Err(status(
                409,
                "There is nothing to commit. The workspace matches its last commit.".into(),
            ));
        }
        data.changes.insert(task_id.into(), Vec::new());
        data.git.undo.remove(task_id);
        let subject = message
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        let graph = data.graphs.entry(task_id.into()).or_default();
        let hash = add_commit(graph, subject.clone());
        let task = data.touch(
            task_id,
            "git.committed",
            json!({ "hash": hash, "subject": subject }),
        )?;
        data.workspace_changed(task_id);
        Ok(CommitAnswer { task, hash })
    }

    /// What the daemon makes when the agent cannot: the title and the files.
    pub fn commit_message(&self, task_id: &str) -> Answer<CommitMessageDraft> {
        let mut data = self.data();
        let task = data.task_mut(task_id)?.clone();
        if data.changed(task_id).is_empty() {
            return Err(status(409, "There is nothing to commit.".into()));
        }
        let subject: String = task.title.chars().take(COMMIT_SUBJECT_MAX).collect();
        let files: Vec<String> = data
            .changed(task_id)
            .iter()
            .map(|entry| format!("- {}", entry.path))
            .collect();
        Ok(CommitMessageDraft {
            message: format!("{subject}\n\n{}", files.join("\n")),
            source: DraftSource::Agent,
            agent: task.agent.unwrap_or_else(|| "claude-code".into()),
            reason: None,
        })
    }

    pub fn rollback(&self, task_id: &str, paths: Vec<String>) -> Answer<RollbackResult> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        for path in &paths {
            if data.change(task_id, path)?.status == ChangeStatus::Conflict {
                return Err(status(
                    409,
                    format!("{path} has a conflict. Resolve it first"),
                ));
            }
        }
        let list = data.changes.entry(task_id.into()).or_default();
        let (removed, kept): (Vec<_>, Vec<_>) = std::mem::take(list)
            .into_iter()
            .partition(|entry| paths.contains(&entry.path));
        *list = kept;
        let files = removed.len() as u64;
        data.git.undo.insert(task_id.into(), removed);
        data.workspace_changed(task_id);
        Ok(RollbackResult { files })
    }

    pub fn undo_rollback(&self, task_id: &str) -> Answer<RollbackResult> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        let Some(back) = data.git.undo.remove(task_id) else {
            return Err(status(409, "Nothing to undo.".into()));
        };
        let files = back.len() as u64;
        let list = data.changes.entry(task_id.into()).or_default();
        list.extend(back);
        list.sort_by(|a, b| a.path.cmp(&b.path));
        data.workspace_changed(task_id);
        Ok(RollbackResult { files })
    }

    pub fn send_to_agent(&self, task_id: &str, text: String) -> Answer<Task> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        if !data.agent_runs(task_id) {
            return Err(status(
                409,
                format!("No agent runs in the terminal of {task_id}. Start the agent first"),
            ));
        }
        data.touch(task_id, "review.commented", json!({ "text": text }))
    }
}

#[cfg(test)]
#[path = "fake_git_tests.rs"]
mod tests;
