//! Deleting a task and everything stored with it (`deleteTask` of `task-service.ts`).

use natamux_proto::tasks::task::Task;
use serde_json::{Value, json};

use crate::shared::event_bus::DaemonMessage;
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::purge::{mark_deleting, purge_task_files, unmark_deleting};
use crate::tasks::store::{Store, logged, parse_task, task_value};

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

    /// Restartable: the marker goes first and last, so a crash in between is finished by
    /// `sweep_orphans` at the next start.
    fn remove_stored_task(&mut self, task_id: &str) -> TaskResult<()> {
        // A write of the task left for after the lock would bring its file back.
        self.sync.settle(task_id);
        let data_dir = self.data_dir();
        mark_deleting(&data_dir, task_id)?;
        purge_task_files(&data_dir, task_id)?;
        self.sync
            .index()
            .delete_task(task_id)
            .map_err(TaskError::Failed)?;
        self.sync.forget(task_id);
        unmark_deleting(&data_dir, task_id)?;
        Ok(())
    }
}
