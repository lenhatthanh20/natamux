//! Deleting a task and everything stored with it (`deleteTask` of `task-service.ts`).

use std::path::Path;

use natamux_proto::tasks::task::Task;
use natamux_store::event_log::event_log_path;
use natamux_store::task_files::{archive_dir, task_file_path};
use serde_json::{Value, json};

use crate::shared::event_bus::DaemonMessage;
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::store::{Store, logged, parse_task, task_value};
use crate::terminals::shell_files::{delete_ended_shells, delete_open_shells};

impl Store {
    /// Removes the task and everything stored with it. A workspace is refused here: the caller
    /// removes that first, so a worktree is never left behind with no task.
    pub fn delete_task(&mut self, task_id: &str) -> TaskResult<Vec<Task>> {
        let task = self.get_task(task_id)?;
        if task.workspace.is_some() {
            return Err(TaskError::Edit(format!("{task_id} still has a workspace")));
        }
        let related = self.detach(task_id)?;
        self.remove_stored_task(task_id)?;
        self.publish(DaemonMessage::TaskDeleted {
            task_id: task_id.into(),
        });
        Ok(related)
    }

    /// A deleted task must not stay as a parent or a dependency of the tasks that remain.
    fn detach(&mut self, task_id: &str) -> TaskResult<Vec<Task>> {
        let mut changed = Vec::new();
        for other in self.list_tasks(None) {
            if other.id == task_id {
                continue;
            }
            let mut fields = Vec::new();
            let mut next = task_value(&other);
            if other.parent_id.as_deref() == Some(task_id) {
                next.insert("parentId".into(), Value::Null);
                fields.push("parentId");
            }
            if other.depends_on.iter().any(|id| id == task_id) {
                let kept: Vec<&String> = other
                    .depends_on
                    .iter()
                    .filter(|id| *id != task_id)
                    .collect();
                next.insert("dependsOn".into(), json!(kept));
                fields.push("dependsOn");
            }
            if fields.is_empty() {
                continue;
            }
            let event = self.event(
                &other.id,
                "system",
                "task.edited",
                json!({ "fields": fields }),
            );
            next.insert("updatedAt".into(), json!(event.at));
            let updated = parse_task(next)?;
            self.sync
                .store(updated.clone(), &[logged(event.clone(), true)], true)?;
            self.publish(DaemonMessage::TaskUpdated {
                task: updated.clone(),
                event,
                accepted: true,
                moved: None,
            });
            changed.push(updated);
        }
        Ok(changed)
    }

    fn remove_stored_task(&mut self, task_id: &str) -> TaskResult<()> {
        let data_dir = self.data_dir();
        remove_file(&task_file_path(&data_dir, task_id))?;
        remove_file(&event_log_path(&data_dir, task_id))?;
        match std::fs::remove_dir_all(archive_dir(&data_dir, task_id)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        delete_ended_shells(&data_dir, task_id)?;
        delete_open_shells(&data_dir, task_id)?;
        remove_file(
            &natamux_store::task_files::tasks_dir(&data_dir).join(format!("{task_id}.usage.json")),
        )?;
        remove_launch_scripts(&data_dir, task_id)?;
        self.sync
            .index
            .delete_task(task_id)
            .map_err(TaskError::Failed)?;
        self.sync.forget(task_id);
        Ok(())
    }
}

fn remove_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// Launch scripts are named agent-launch-T-0001.sh and agent-launch-T-0001-2.sh.
fn remove_launch_scripts(data_dir: &Path, task_id: &str) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let exact = format!("agent-launch-{task_id}.sh");
    let prefix = format!("agent-launch-{task_id}-");
    for entry in entries {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name == exact || (name.starts_with(&prefix) && name.ends_with(".sh")) {
            remove_file(&data_dir.join(name))?;
        }
    }
    Ok(())
}
