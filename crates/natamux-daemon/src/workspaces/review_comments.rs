//! Review comments of a task (`/tasks/:id/review-comments`): kept in `review-comments.json` in the
//! task's archive folder, pinned to their code on each list, and sent to the agent in one batch.
//!
//! Everything here blocks (files, git, the terminal): routes run it with `spawn_blocking`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use natamux_core::git::review_file::{review_comments_md, send_prompt};
use natamux_core::git::review_pin::{pin, quote_lines};
use natamux_core::js::iso_now;
use natamux_git::file_diff::read_side;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::review_comments::{
    CommentSide, CommentState, MAX_COMMENTS, NewReviewComment, REVIEW_COMMENTS_FILE,
    ReviewComment, ReviewCommentsFile,
};
use natamux_stages::files::write_stage_file;
use natamux_store::task_files::{archive_dir, write_atomic};

use crate::http::routes::agents::paste_prompt;
use crate::http::server::AppState;
use crate::stages::errors::{StageError, StageResult, action};
use crate::tasks::errors::TaskError;

const STORE_FILE: &str = "review-comments.json";

// One lock for the whole daemon: a read-change-write of one task never runs beside another.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn failed(text: String) -> StageError {
    StageError::Task(TaskError::Failed(text))
}

fn store_path(state: &AppState, task_id: &str) -> PathBuf {
    archive_dir(&state.data_dir, task_id).join(STORE_FILE)
}

fn load(path: &Path) -> StageResult<ReviewCommentsFile> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| failed(format!("{} is not valid: {error}", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ReviewCommentsFile::default())
        }
        Err(error) => Err(error.into()),
    }
}

fn save(path: &Path, file: &ReviewCommentsFile) -> StageResult<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let bytes = serde_json::to_vec_pretty(file).map_err(|error| failed(error.to_string()))?;
    write_atomic(path, &bytes)?;
    Ok(())
}

fn missing(cid: &str) -> StageError {
    // A 404 with a free text: `RepoNotFound` is the one task error that carries its own words.
    StageError::Task(TaskError::RepoNotFound(format!(
        "Comment {cid} does not exist"
    )))
}

fn number(comment: &ReviewComment) -> u32 {
    comment.id.get(1..).and_then(|n| n.parse().ok()).unwrap_or(0)
}

fn sorted(mut comments: Vec<ReviewComment>) -> Vec<ReviewComment> {
    comments.sort_by(|a, b| (&a.path, a.start, number(a)).cmp(&(&b.path, b.start, number(b))));
    comments
}

/// Where a path was in the last commit, for the paths the change list shows as renamed.
fn renames(state: &AppState, task_id: &str) -> HashMap<String, String> {
    let changes = state.workspaces.changes(task_id, false).unwrap_or_default();
    changes
        .into_iter()
        .filter_map(|change| Some((change.path, change.original_path?)))
        .collect()
}

/// Every comment as the clients see it: moved to where its quote is now, `outdated` set. The
/// stored lines are moved too; the caller saves when this says so. Each side is read once.
fn pin_all(
    state: &AppState,
    task: &Task,
    file: &mut ReviewCommentsFile,
) -> (Vec<ReviewComment>, bool) {
    let workspace = task.workspace.as_deref().map(Path::new);
    let renamed = if workspace.is_some() {
        renames(state, &task.id)
    } else {
        HashMap::new()
    };
    let mut texts: HashMap<(String, CommentSide), Option<String>> = HashMap::new();
    let mut moved = false;
    let mut out = Vec::with_capacity(file.comments.len());
    for comment in &mut file.comments {
        let key = (comment.path.clone(), comment.side);
        let text = texts.entry(key).or_insert_with(|| {
            let from = renamed.get(&comment.path).map(String::as_str);
            // A side git cannot read is treated as gone: the comment shows as outdated.
            read_side(workspace?, &comment.path, from, comment.side)
                .ok()
                .flatten()
        });
        let at = pin(comment.start, comment.end, &comment.quote, text.as_deref());
        if (at.start, at.end) != (comment.start, comment.end) {
            comment.start = at.start;
            comment.end = at.end;
            moved = true;
        }
        out.push(ReviewComment {
            outdated: at.outdated,
            ..comment.clone()
        });
    }
    (out, moved)
}

/// The comments of the task, in order. `moved`: a pin moved and was saved, so dashboards must
/// hear of it.
pub fn list(
    state: &AppState,
    task_id: &str,
    only_path: Option<&str>,
) -> StageResult<(Vec<ReviewComment>, bool)> {
    let _guard = lock();
    let task = state.tasks.get_task(task_id)?;
    let path = store_path(state, task_id);
    let mut file = load(&path)?;
    let (mut comments, moved) = pin_all(state, &task, &mut file);
    if moved {
        save(&path, &file)?;
    }
    if let Some(only) = only_path {
        comments.retain(|comment| comment.path == only);
    }
    Ok((sorted(comments), moved))
}

pub fn add(
    state: &AppState,
    task_id: &str,
    input: &NewReviewComment,
) -> StageResult<ReviewComment> {
    let _guard = lock();
    let task = state.tasks.get_task(task_id)?;
    let Some(workspace) = task.workspace.as_deref().map(Path::new) else {
        return Err(action(format!("{task_id} has no workspace")));
    };
    let from = renames(state, task_id).get(&input.path).cloned();
    let text = read_side(workspace, &input.path, from.as_deref(), input.side)
        .map_err(|error| StageError::Input(error.to_string()))?
        .ok_or_else(|| {
            StageError::Input(format!(
                "{} cannot be read on its {} side",
                input.path,
                input.side.name()
            ))
        })?;
    let (start, end) = (input.start, input.end);
    let quote = quote_lines(&text, start, end).ok_or_else(|| {
        StageError::Input(format!(
            "Lines {start} to {end} are not in {} ({} side)",
            input.path,
            input.side.name()
        ))
    })?;
    let path = store_path(state, task_id);
    let mut file = load(&path)?;
    if file.comments.len() >= MAX_COMMENTS {
        return Err(StageError::Input(format!(
            "A task keeps at most {MAX_COMMENTS} review comments"
        )));
    }
    file.next += 1;
    let comment = ReviewComment {
        id: format!("c{}", file.next),
        path: input.path.clone(),
        side: input.side,
        start,
        end,
        quote,
        body: input.body.clone(),
        state: CommentState::Draft,
        created_at: iso_now(),
        sent_at: None,
        resolved_at: None,
        outdated: false,
    };
    file.comments.push(comment.clone());
    save(&path, &file)?;
    Ok(comment)
}

/// A new text (a draft only) and/or a new state (sent <-> resolved only).
pub fn patch(
    state: &AppState,
    task_id: &str,
    cid: &str,
    body: Option<&str>,
    new_state: Option<CommentState>,
) -> StageResult<ReviewComment> {
    let _guard = lock();
    state.tasks.get_task(task_id)?;
    let path = store_path(state, task_id);
    let mut file = load(&path)?;
    let slot = file
        .comments
        .iter_mut()
        .find(|comment| comment.id == cid)
        .ok_or_else(|| missing(cid))?;
    let mut next = slot.clone();
    if let Some(body) = body {
        if next.state != CommentState::Draft {
            return Err(action(format!("{cid} was sent, so its text cannot change")));
        }
        next.body = body.to_string();
    }
    match (next.state, new_state) {
        (_, None) => {}
        (CommentState::Sent, Some(CommentState::Resolved)) => {
            next.state = CommentState::Resolved;
            next.resolved_at = Some(iso_now());
        }
        (CommentState::Resolved, Some(CommentState::Sent)) => {
            next.state = CommentState::Sent;
            next.resolved_at = None;
        }
        (from, Some(to)) => {
            return Err(action(format!(
                "{cid} cannot go from {} to {}",
                from.name(),
                to.name()
            )));
        }
    }
    *slot = next.clone();
    save(&path, &file)?;
    Ok(next)
}

pub fn delete(state: &AppState, task_id: &str, cid: &str) -> StageResult<()> {
    let _guard = lock();
    state.tasks.get_task(task_id)?;
    let path = store_path(state, task_id);
    let mut file = load(&path)?;
    let before = file.comments.len();
    file.comments.retain(|comment| comment.id != cid);
    if file.comments.len() == before {
        return Err(missing(cid));
    }
    save(&path, &file)
}

/// Writes `.natamux-task/review-comments.md`, puts one prompt in front of the agent, and only
/// then marks the drafts sent. A paste that fails leaves them drafts.
pub fn send(state: &AppState, task_id: &str) -> StageResult<(Task, Vec<ReviewComment>)> {
    let _guard = lock();
    let task = state.tasks.get_task(task_id)?;
    let path = store_path(state, task_id);
    let mut file = load(&path)?;
    let is_draft = |comment: &ReviewComment| comment.state == CommentState::Draft;
    if !file.comments.iter().any(is_draft) {
        return Err(action("No draft comments to send"));
    }
    let Some(workspace) = task.workspace.as_deref() else {
        return Err(action(format!("{task_id} has no workspace")));
    };
    // The moved pins are saved with the new states below.
    let (mut all, _) = pin_all(state, &task, &mut file);
    let ids: Vec<String> = all
        .iter()
        .filter(|comment| is_draft(comment))
        .map(|comment| comment.id.clone())
        .collect();
    let text = review_comments_md(&all);
    write_stage_file(
        &state.data_dir,
        task_id,
        Some(workspace),
        REVIEW_COMMENTS_FILE,
        &text,
    )
    .map_err(|error| StageError::File(error.to_string()))?;
    paste_prompt(state, task_id, &send_prompt(ids.len()))?;

    let now = iso_now();
    for comment in file.comments.iter_mut().chain(all.iter_mut()) {
        if ids.contains(&comment.id) {
            comment.state = CommentState::Sent;
            comment.sent_at = Some(now.clone());
        }
    }
    save(&path, &file)?;
    let task = state.tasks.record_review_comments(task_id, &ids)?;
    Ok((task, sorted(all)))
}

#[cfg(test)]
#[path = "review_comments_tests.rs"]
mod tests;
