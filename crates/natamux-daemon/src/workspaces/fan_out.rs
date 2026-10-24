//! `fan-out.ts`: copies of a task, to let several agents try the same work.

use std::path::PathBuf;
use std::sync::Arc;

use natamux_proto::tasks::task::Task;
use natamux_store::attachments::read_attachments;
use serde_json::json;

use crate::stages::errors::{StageError, StageResult};
use crate::tasks::service::TaskService;
use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};
use crate::workspaces::service::WorkspaceService;

pub const MAX_TRIES: usize = 5;

/// The longest title of a task, in UTF-16 units (Node's `slice(0, 200)`).
const MAX_TITLE_UNITS: usize = 200;

/// Starts an agent by hand in a task: `StageRunner::start_by_hand` (task id, agent).
pub type StartByHand = Arc<dyn Fn(&str, &str) -> StageResult<Task> + Send + Sync>;

pub struct FanOutParts {
    pub tasks: TaskService,
    pub workspaces: Arc<WorkspaceService>,
    pub start_by_hand: StartByHand,
    /// Where the attachments of the task are, to copy them to its tries.
    pub data_dir: PathBuf,
}

/// The label that groups a task and its tries, so a search for it shows them side by side.
pub fn try_label(task_id: &str) -> String {
    format!("tries-{}", task_id.to_lowercase())
}

/// A failed stage call as a workspace error: a task error keeps its status, the rest is a 409.
pub fn stage_failure(error: StageError) -> WorkspaceError {
    match error {
        StageError::Task(error) => WorkspaceError::Task(error),
        other => WorkspaceError::Conflict(other.to_string()),
    }
}

fn cut_title(title: &str) -> String {
    let mut units = 0;
    title
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= MAX_TITLE_UNITS
        })
        .collect()
}

/// Makes `count` copies of a task. With `start`, each copy gets a workspace and Claude Code at
/// once. The copies are compared by their diffs; the best one is kept and the others are
/// cancelled by hand.
pub fn fan_out(
    parts: &FanOutParts,
    task_id: &str,
    count: usize,
    start: bool,
) -> WorkspaceResult<Vec<Task>> {
    let source = parts.tasks.get_task(task_id)?;
    let label = try_label(&source.id);
    let attachments = read_attachments(&parts.data_dir, &source.id)
        .map_err(|error| WorkspaceError::Io(error.to_string()))?;
    if !source.labels.contains(&label) {
        let mut labels = source.labels.clone();
        labels.push(label.clone());
        parts
            .tasks
            .update_task(&source.id, Some(&json!({ "labels": labels })))?;
    }

    let tries = parts
        .tasks
        .list_tasks(None)?
        .into_iter()
        .filter(|task| task.labels.contains(&label) && task.id != source.id)
        .count();
    // The task is try 1, and a second fan out goes on from the tries made before.
    let first = tries + 2;
    let mut labels = source.labels.clone();
    labels.retain(|each| *each != label);
    labels.push(label.clone());
    let mut created = Vec::new();
    for number in first..count + first {
        let mut task = parts.tasks.create_task(
            Some(&json!({
                "title": cut_title(&format!("{} (try {number})", source.title)),
                "description": source.description,
                "priority": source.priority,
                "labels": labels,
                "dependsOn": source.depends_on,
                "repo": source.repo,
            })),
            &attachments,
            // The title of the source lost its id already; a word left at its start is not an id.
            false,
        )?;
        if start {
            parts.workspaces.create(&task.id, None)?;
            task = (parts.start_by_hand)(&task.id, "claude-code").map_err(stage_failure)?;
        }
        created.push(task);
    }
    Ok(created)
}
