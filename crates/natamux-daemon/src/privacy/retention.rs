//! Retention (`data.retentionDays`): at start and every 24 h, finished tasks older than N days go
//! through the normal delete, without force. A task with uncommitted changes, or with commits
//! that only its branch holds, is skipped and logged `refused`, so retention never loses work.
//! 0 (the default) deletes nothing.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use natamux_core::js::parse_iso_millis;
use natamux_core::security::data_in;
use natamux_git::exec::git;
use natamux_proto::tasks::task::{Task, TaskState};

use crate::http::server::AppState;
use crate::privacy::audit;

const DAY_MS: i64 = 86_400_000;
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetentionReport {
    pub deleted: Vec<String>,
    pub refused: Vec<String>,
}

/// Done or cancelled, and not changed for more than `days` days.
pub fn is_due(task: &Task, now_ms: i64, days: u32) -> bool {
    if days == 0 || !matches!(task.state, TaskState::Done | TaskState::Cancelled) {
        return false;
    }
    parse_iso_millis(&task.updated_at)
        .is_some_and(|changed| now_ms.saturating_sub(changed) > i64::from(days) * DAY_MS)
}

/// The folder git runs in and the tip whose commits the delete would drop: the worktree's HEAD,
/// or the branch in the project when the worktree is gone. None: nothing to lose.
fn tip_of(state: &AppState, task: &Task) -> Option<(PathBuf, String)> {
    if let Some(workspace) = task.workspace.as_deref().map(Path::new)
        && workspace.is_dir()
    {
        return Some((workspace.to_path_buf(), "HEAD".to_string()));
    }
    let branch = task.branch.as_deref()?;
    // A removed project keeps no branch the delete could drop.
    let repo = PathBuf::from(state.repos.path_of(task.repo.as_deref()).ok()?);
    let name = format!("refs/heads/{branch}");
    let exists = git(
        &repo,
        &["rev-parse", "--verify", "--quiet", &name],
        None,
        &[],
    )
    .is_ok();
    exists.then_some((repo, name))
}

/// True when the delete would drop commits no other branch, remote branch or tag holds. A git
/// error proves nothing, so it counts as true.
pub fn has_unique_commits(state: &AppState, task: &Task) -> bool {
    let Some((cwd, tip)) = tip_of(state, task) else {
        return false;
    };
    // `--exclude` before `--branches` takes the name without `refs/heads/`. The branch the agent
    // switched to is in `--branches` too, and the delete would drop it, so it is excluded as well.
    let mut names: Vec<String> = task.branch.iter().cloned().collect();
    if tip == "HEAD"
        && let Ok(now) = git(&cwd, &["symbolic-ref", "--short", "-q", "HEAD"], None, &[])
    {
        names.push(now.trim().to_string());
    }
    let excludes: Vec<String> = names
        .iter()
        .map(|name| format!("--exclude={name}"))
        .collect();
    let mut args = vec!["rev-list", "--count", tip.as_str(), "--not"];
    args.extend(excludes.iter().map(String::as_str));
    args.extend(["--branches", "--remotes", "--tags"]);
    !git(&cwd, &args, None, &[]).is_ok_and(|count| count.trim() == "0")
}

fn delete_one(state: &AppState, task: &Task) -> bool {
    if has_unique_commits(state, task) {
        return false;
    }
    let deleted = state.workspaces.delete_task_and_workspace(&task.id, false);
    if deleted.is_ok() {
        state.terminals.revoke_agent(&task.id);
    }
    deleted.is_ok()
}

/// One run. Safe to run again: a deleted task is gone, a refused one is refused again.
pub fn run_once(state: &AppState, days: u32, now_ms: i64) -> RetentionReport {
    let mut report = RetentionReport::default();
    if days == 0 {
        return report;
    }
    let due: Vec<Task> = state
        .tasks
        .list_tasks(None)
        .unwrap_or_default()
        .into_iter()
        .filter(|task| is_due(task, now_ms, days))
        .collect();
    for task in &due {
        let ok = delete_one(state, task);
        let result = if ok { "ok" } else { "refused" };
        audit::write(&state.data_dir, "task.delete", &task.id, result);
        let list = if ok {
            &mut report.deleted
        } else {
            &mut report.refused
        };
        list.push(task.id.clone());
    }
    let summary = format!("deleted:{}", report.deleted.len());
    let result = format!("refused:{}", report.refused.len());
    audit::write(&state.data_dir, "retention.run", &summary, &result);
    report
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(0)
}

/// Runs now and every 24 h while the daemon lives. The days are read again each time, so a
/// change of the setting applies at the next run.
pub fn start(state: &Arc<AppState>) {
    let state = Arc::downgrade(state);
    let spawned = std::thread::Builder::new()
        .name("retention".into())
        .spawn(move || {
            loop {
                let Some(state) = state.upgrade() else {
                    return;
                };
                let days = data_in(&state.data_dir).retention_days;
                let report = run_once(&state, days, now_ms());
                if !report.deleted.is_empty() || !report.refused.is_empty() {
                    eprintln!(
                        "Retention deleted {} finished tasks and kept {} with work in them.",
                        report.deleted.len(),
                        report.refused.len()
                    );
                }
                drop(state);
                std::thread::sleep(EVERY);
            }
        });
    if let Err(error) = spawned {
        eprintln!("Retention does not run: {error}");
    }
}
