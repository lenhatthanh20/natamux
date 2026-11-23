//! The approval of a CI push (F4): what a CI round may send, recorded before the round starts,
//! and a person's approval under `ciPush: approved-snapshot`. Retry and resume reuse it.

use std::path::Path;

use natamux_core::security::{CI_FORBID_REASON, CiMode, ci_mode};
use natamux_git::ci_snapshot::{
    head_snapshot, new_commits, pin, remote_url, tree_of, unpin_task, working_files,
};
use natamux_git::commit_push::{delete_remote_ref, snapshot_paths};
use natamux_git::exec::GitError;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::Task;
use natamux_stages::ci::{CiTarget, ci_approval_of, ci_branch_of};
use serde_json::{Map, Value, json};

use crate::stages::errors::{StageResult, action};
use crate::stages::run::{StageRun, key};

/// The names of files left out that an event keeps; the count says how many there were.
const SHOWN_FILES: usize = 20;

pub const NO_APPROVAL: &str = "The CI check has no approval for this round, so nothing was pushed. \
     Report the stage again to start a new CI check.";

/// What a CI round of `mode` sends from `worktree`, as the payload of its approval event. The
/// commit is pinned by a local ref until the task ends.
pub fn build_approval(
    worktree: &Path,
    task_id: &str,
    stage: &str,
    branch: &str,
    mode: CiMode,
) -> Result<Map<String, Value>, GitError> {
    let files = working_files(worktree)?;
    let (commit, tree, sent, not_sent) = if mode == CiMode::ApprovedCommit {
        let (commit, tree) = head_snapshot(worktree)?;
        (
            commit,
            tree,
            Vec::new(),
            [files.tracked, files.untracked].concat(),
        )
    } else {
        let message = format!("Natamux CI check of {branch}");
        let commit = snapshot_paths(worktree, &message, &files.tracked)?;
        let tree = tree_of(worktree, &commit)?;
        (commit, tree, files.tracked, files.untracked)
    };
    pin(worktree, task_id, stage, &commit)?;
    let shown: Vec<&String> = not_sent.iter().take(SHOWN_FILES).collect();
    let payload = json!({
        "stage": stage,
        "mode": mode.as_str(),
        // A repository without origin has no URL; the push then fails on its own.
        "remote": remote_url(worktree).unwrap_or_default(),
        "branch": ci_branch_of(branch),
        "commit": commit,
        "tree": tree,
        "files": sent,
        "notSent": shown,
        "notSentCount": not_sent.len(),
        "newCommits": new_commits(worktree).unwrap_or(0),
    });
    Ok(match payload {
        Value::Object(map) => map,
        _ => Map::new(),
    })
}

/// The line the stage card shows about what CI does not get.
pub fn not_sent_note(payload: &Map<String, Value>) -> Option<String> {
    let count = payload
        .get("notSentCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mode = payload.get("mode").and_then(Value::as_str);
    if mode == Some(CiMode::ApprovedCommit.as_str()) {
        let commit = payload.get("commit").and_then(Value::as_str).unwrap_or("");
        let short: String = commit.chars().take(7).collect();
        let rest = match count {
            0 => String::new(),
            1 => " 1 uncommitted file is not sent.".into(),
            n => format!(" {n} uncommitted files are not sent."),
        };
        return Some(format!("CI checks the last commit ({short}).{rest}"));
    }
    if count == 0 {
        return None;
    }
    let names: Vec<&str> = payload
        .get("notSent")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let more = if (names.len() as u64) < count {
        ", …"
    } else {
        ""
    };
    let what = if count == 1 {
        "1 untracked file is".to_string()
    } else {
        format!("{count} untracked files are")
    };
    Some(format!("{what} not sent: {}{more}", names.join(", ")))
}

/// The approval a person still has to give for the stage: its latest `stage.ci-approval-needed`
/// with nothing after it that ended or answered it.
pub fn pending_approval(events: &[StoredEvent], stage: &str) -> Option<Map<String, Value>> {
    let mut pending = None;
    for stored in events {
        let event = &stored.event;
        if event.payload.get("stage").and_then(Value::as_str) != Some(stage) {
            continue;
        }
        match event.kind.as_str() {
            "stage.ci-approval-needed" => pending = Some(event.payload.clone()),
            "stage.ci-approved" | "stage.started" | "stage.checked" | "stage.check-failed" => {
                pending = None;
            }
            _ => {}
        }
    }
    pending
}

/// At task end the pins go, and the CI branch of origin too, unless CI pushes are forbidden:
/// then no remote call is made at all. Errors are only logged.
pub fn clean_ci_refs(folder: &Path, task_id: &str, ref_name: &str, forbid: bool) {
    // An empty folder would run git in the daemon's own folder.
    if folder.as_os_str().is_empty() {
        return;
    }
    if let Err(error) = unpin_task(folder, task_id) {
        eprintln!("Could not remove the CI pins of {task_id}: {error}");
    }
    if forbid {
        return;
    }
    if let Err(error) = delete_remote_ref(folder, ref_name) {
        eprintln!("Could not delete the CI branch {ref_name} of {task_id}: {error}");
    }
}

impl StageRun {
    /// The start of a new CI round. Under `forbid` it fails for good; under `approved-snapshot` it
    /// waits for a person; otherwise the approval and the start are recorded. True when the
    /// round runs now.
    pub(crate) fn open_ci_round(
        &self,
        task: &Task,
        stage_id: &str,
        passes: &[String],
    ) -> StageResult<bool> {
        let mode = ci_mode();
        if mode == CiMode::Forbid {
            self.block_ci(&task.id, stage_id, CI_FORBID_REASON)?;
            return Ok(false);
        }
        let branch = task.branch.clone().unwrap_or_default();
        let Some(workspace) = task.workspace.as_deref() else {
            // The round fails it with the reason.
            let started = json!({ "stage": stage_id, "branch": ci_branch_of(&branch) });
            self.record(&task.id, "stage.ci-started", started)?;
            return Ok(true);
        };
        let built = build_approval(Path::new(workspace), &task.id, stage_id, &branch, mode);
        let mut payload = match built {
            Ok(payload) => payload,
            Err(error) => {
                let reason = format!("Could not prepare the CI check: {error}");
                self.fail_checks(&task.id, stage_id, &[reason], true, None)?;
                return Ok(false);
            }
        };
        if mode == CiMode::ApprovedSnapshot {
            payload.insert("passes".into(), json!(passes));
            self.record(&task.id, "stage.ci-approval-needed", Value::Object(payload))?;
            return Ok(false);
        }
        payload.insert("approver".into(), json!("auto"));
        self.start_approved(&task.id, stage_id, payload)?;
        Ok(true)
    }

    /// Records the approval, then the start that names it.
    fn start_approved(
        &self,
        task_id: &str,
        stage_id: &str,
        payload: Map<String, Value>,
    ) -> StageResult<()> {
        let note = not_sent_note(&payload);
        let branch = payload.get("branch").cloned().unwrap_or(Value::Null);
        self.record(task_id, "stage.ci-approved", Value::Object(payload))?;
        let approval = ci_approval_of(&self.events(task_id), stage_id).map(|found| found.seq);
        let mut started = json!({ "stage": stage_id, "branch": branch, "approval": approval });
        if let Some(note) = note {
            started["note"] = json!(note);
        }
        self.record(task_id, "stage.ci-started", started)?;
        Ok(())
    }

    /// A person approved the exact tree shown (`approved-snapshot`): the round starts.
    pub fn approve_ci(&self, task_id: &str, stage_id: &str, tree: &str) -> StageResult<Task> {
        let Some(mut payload) = pending_approval(&self.events(task_id), stage_id) else {
            return Err(action(format!(
                "No CI push of the {stage_id} stage waits for an approval."
            )));
        };
        let shown = payload.get("tree").and_then(Value::as_str).unwrap_or("");
        if shown != tree {
            return Err(action(format!(
                "The tree to send is {shown}, not {tree}. Check it and approve that one."
            )));
        }
        if self.state().ci_rounds.contains(&key(task_id, stage_id)) {
            return Err(action("The CI check of this stage runs already."));
        }
        let passes: Vec<String> = payload
            .remove("passes")
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        payload.insert("approver".into(), json!("human"));
        self.start_approved(task_id, stage_id, payload)?;
        let seq = self.last_start(task_id, stage_id).seq;
        self.start_ci_round(task_id, stage_id, seq, passes);
        self.task(task_id)
    }

    /// What the round may push: the approval made after the stage's last start.
    pub(crate) fn approved_target(
        &self,
        task_id: &str,
        stage_id: &str,
        seq: i64,
    ) -> Option<CiTarget> {
        ci_approval_of(&self.events(task_id), stage_id)
            .filter(|found| i64::try_from(found.seq).is_ok_and(|at| at > seq))
            .map(|found| found.target)
    }

    /// A CI round that may not run: logged with its reason, and a failure no agent can fix.
    pub(crate) fn block_ci(&self, task_id: &str, stage_id: &str, reason: &str) -> StageResult<()> {
        crate::privacy::audit::write(&self.options.data_dir, "ci.blocked", task_id, "blocked");
        self.record(
            task_id,
            "stage.ci-blocked",
            json!({ "stage": stage_id, "reason": reason }),
        )?;
        self.fail_checks(task_id, stage_id, &[reason.to_string()], true, None)
    }
}

#[cfg(test)]
#[path = "ci_approval_tests.rs"]
mod tests;
