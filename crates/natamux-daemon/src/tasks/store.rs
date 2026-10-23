//! The work of the task service under its lock (`task-service.ts`). Messages are queued on the
//! bus here and sent by `TaskService` after the lock is let go.

use std::path::PathBuf;
use std::sync::Arc;

use natamux_core::events::apply::apply_task_event;
use natamux_core::events::apply::complete_event;
use natamux_core::events::schema::validate_task_event;
use natamux_core::js::iso_from_millis;
use natamux_core::stages::position::first_position;
use natamux_core::stages::terminal_lane::pinned_terminals_of;
use natamux_core::tasks::dependencies::makes_cycle;
use natamux_core::tasks::inputs::{apply_event_input, create_task_input, update_task_input};
use natamux_core::tasks::task::{split_task_id, validate_task};
use natamux_proto::events::task_event::{StoredEvent, TaskEvent};
use natamux_proto::stages::resolve::{active_steps, resolve_steps, step_refs_of};
use natamux_proto::stages::schema::{Step, StepRef};
use natamux_proto::tasks::task::{EDITABLE_FIELDS, Task, TaskState};
use natamux_store::attachments::{AttachmentFile, save_attachments};
use natamux_store::event_log::LoggedEvent;
use natamux_store::task_files::{list_task_ids, read_task_file};
use natamux_store::task_sync::TaskSync;
use serde_json::{Map, Value, json};

use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::jobs::Clock;
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::helpers::{background_since_after, compare_text, move_task, position_of};

/// The stages of a repository (`.natamux.json`); None is the default project.
pub type SettingsOf = Arc<dyn Fn(Option<&str>) -> Vec<Step> + Send + Sync>;

/// What `applyEvent` gives.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub task: Task,
    pub accepted: bool,
    pub moved: bool,
}

pub struct Store {
    pub sync: TaskSync,
    pub bus: Arc<EventBus>,
    pub settings_of: SettingsOf,
    pub now: Clock,
}

pub fn logged(event: TaskEvent, accepted: bool) -> LoggedEvent {
    LoggedEvent { event, accepted }
}

pub fn event_at(task_id: &str, source: &str, kind: &str, payload: Value, at: String) -> TaskEvent {
    TaskEvent {
        task_id: task_id.into(),
        source: source.into(),
        kind: kind.into(),
        at,
        payload: match payload {
            Value::Object(map) => map,
            _ => Map::new(),
        },
    }
}

pub fn task_value(task: &Task) -> Map<String, Value> {
    match serde_json::to_value(task) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// A task built in code, checked as `taskSchema.parse` does.
pub fn parse_task(fields: Map<String, Value>) -> TaskResult<Task> {
    Ok(validate_task(&Value::Object(fields))?)
}

impl Store {
    pub fn data_dir(&self) -> PathBuf {
        self.sync.data_dir().to_path_buf()
    }

    pub fn timestamp(&self) -> String {
        iso_from_millis((self.now)())
    }

    /// An event at the time now. Each call reads the clock once, as Node's `timestamp()`.
    pub fn event(&self, task_id: &str, source: &str, kind: &str, payload: Value) -> TaskEvent {
        event_at(task_id, source, kind, payload, self.timestamp())
    }

    pub fn publish(&self, message: DaemonMessage) {
        self.bus.enqueue(message);
    }

    /// The steps of a task, with each stage's settings from its repository.
    pub fn steps_of(&self, task: &Task) -> Vec<Step> {
        let refs: Vec<StepRef> = task
            .stages
            .iter()
            .filter_map(|each| serde_json::from_value(each.clone()).ok())
            .collect();
        resolve_steps(&refs, &(self.settings_of)(task.repo.as_deref()))
    }

    pub fn get_task(&mut self, task_id: &str) -> TaskResult<Task> {
        self.sync.check_file(task_id);
        self.find_task(task_id)?
            .ok_or_else(|| TaskError::NotFound(task_id.into()))
    }

    /// Before init the map holds only the tasks this service wrote or read, so a task is looked
    /// for on disk too.
    pub fn find_task(&mut self, task_id: &str) -> TaskResult<Option<Task>> {
        if let Some(task) = self.sync.tasks.get(task_id) {
            return Ok(Some(task.clone()));
        }
        if self.sync.started {
            return Ok(None);
        }
        let read = read_task_file(&self.data_dir(), task_id)
            .map_err(|error| TaskError::Failed(error.to_string()))?;
        if let Some(task) = &read {
            self.sync.tasks.insert(task_id.into(), task.clone());
        }
        Ok(read)
    }

    /// Newest first, as the index lists them.
    pub fn list_tasks(&mut self, state: Option<TaskState>) -> Vec<Task> {
        self.sync.check_folder();
        let mut tasks: Vec<Task> = self
            .sync
            .tasks
            .values()
            .filter(|task| state.is_none_or(|state| task.state == state))
            .cloned()
            .collect();
        tasks.sort_by(|a, b| {
            compare_text(&b.updated_at, &a.updated_at).then_with(|| compare_text(&a.id, &b.id))
        });
        tasks
    }

    pub fn list_events(&mut self, task_id: &str) -> TaskResult<Vec<StoredEvent>> {
        self.get_task(task_id)?;
        self.sync
            .index
            .list_events(task_id)
            .map_err(TaskError::Failed)
    }

    /// The attachments are written before the task is published. A title that starts with an id,
    /// such as "ABC-12: Fix it", picks the id, unless `split_id` is false.
    pub fn create_task(
        &mut self,
        raw: Option<&Value>,
        attachments: &[AttachmentFile],
        split_id: bool,
    ) -> TaskResult<Task> {
        let mut input = create_task_input(raw)?;
        let parent_id = input
            .get("parentId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let parent = match parent_id {
            Some(parent_id) => Some(self.check_parent(&parent_id)?),
            None => None,
        };
        if let Some(parent) = &parent {
            check_not_on_parent(&parent.id, &ids(input.get("dependsOn")))?;
            // A subtask is part of its parent's work, so it is in the same project.
            let no_repo = input.get("repo").is_none_or(|repo| repo.is_null());
            if no_repo {
                input.insert("repo".into(), json!(parent.repo));
            }
        }
        let title = input
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let (id, title) = if split_id {
            split_task_id(&title)
        } else {
            (None, title)
        };
        if let Some(id) = &id {
            let on_disk = read_task_file(&self.data_dir(), id)
                .map_err(|error| TaskError::Failed(error.to_string()))?;
            if self.sync.tasks.contains_key(id) || on_disk.is_some() {
                return Err(TaskError::Edit(format!("Task id {id} is already used")));
            }
        }
        let at = self.timestamp();
        let task = self.new_task(input, id, title, &at)?;
        self.store_new_task(&task, attachments, &at)?;
        self.publish(DaemonMessage::TaskCreated { task: task.clone() });
        Ok(task)
    }

    /// The new task at the first step of its project's stages.
    fn new_task(
        &self,
        mut input: Map<String, Value>,
        id: Option<String>,
        title: String,
        at: &str,
    ) -> TaskResult<Task> {
        // Stages that are off stay in the settings, but a new task does not get them.
        let repo = input
            .get("repo")
            .and_then(Value::as_str)
            .map(str::to_string);
        let steps = active_steps(&(self.settings_of)(repo.as_deref()));
        let start = first_position(&steps);
        let lanes: Map<String, Value> = start
            .lanes
            .iter()
            .map(|(lane, state)| (lane.clone(), json!(state.name())))
            .collect();
        let id = match id {
            Some(id) => id,
            None => self.next_task_id(),
        };
        for (key, value) in [
            ("id", json!(id)),
            ("title", json!(title)),
            ("stage", json!(start.stage)),
            ("state", json!(start.state.name())),
            ("lanes", Value::Object(lanes)),
            (
                "stages",
                serde_json::to_value(step_refs_of(&steps)).unwrap_or(json!([])),
            ),
            ("stageSince", json!(at)),
            ("createdAt", json!(at)),
            ("updatedAt", json!(at)),
        ] {
            input.insert(key.into(), value);
        }
        parse_task(input)
    }

    /// Stores a new task, its creation and its attachments.
    fn store_new_task(
        &mut self,
        task: &Task,
        attachments: &[AttachmentFile],
        at: &str,
    ) -> TaskResult<()> {
        let at = at.to_string();
        let created = event_at(&task.id, "system", "task.created", json!({}), at.clone());
        self.sync
            .store(task.clone(), &[logged(created, true)], true)?;
        save_attachments(&self.data_dir(), &task.id, attachments)?;
        if !attachments.is_empty() {
            let added: Vec<LoggedEvent> = attachments
                .iter()
                .map(|file| {
                    let payload = json!({ "name": file.name, "size": file.bytes.len() });
                    logged(
                        event_at(&task.id, "system", "attachment.added", payload, at.clone()),
                        true,
                    )
                })
                .collect();
            self.sync.store(task.clone(), &added, false)?;
        }
        Ok(())
    }

    /// Changes the details of a task. Logs `task.edited` with the names of the fields that
    /// changed, and writes nothing when none did.
    pub fn update_task(&mut self, task_id: &str, raw: Option<&Value>) -> TaskResult<Task> {
        let task = self.get_task(task_id)?;
        let input = update_task_input(raw)?;
        if let Some(depends_on) = input.get("dependsOn") {
            let depends_on = ids(Some(depends_on));
            self.check_dependencies(task_id, &depends_on)?;
            if let Some(parent) = &task.parent_id {
                check_not_on_parent(parent, &depends_on)?;
            }
        }
        let before = task_value(&task);
        let mut next = before.clone();
        for (key, value) in input {
            next.insert(key, value);
        }
        let changed: Vec<&str> = EDITABLE_FIELDS
            .into_iter()
            .filter(|field| next.get(*field) != before.get(*field))
            .collect();
        if changed.is_empty() {
            return Ok(task);
        }
        let event = self.event(
            task_id,
            "human",
            "task.edited",
            json!({ "fields": changed }),
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
        Ok(updated)
    }

    /// Subtasks are one level deep: the parent must exist and must not be a subtask itself.
    fn check_parent(&mut self, parent_id: &str) -> TaskResult<Task> {
        let parent = self
            .find_task(parent_id)?
            .ok_or_else(|| TaskError::Edit(format!("No such task: {parent_id}")))?;
        if parent.parent_id.is_some() {
            return Err(TaskError::Edit(format!(
                "{parent_id} is a subtask; a subtask cannot have subtasks"
            )));
        }
        Ok(parent)
    }

    fn check_dependencies(&mut self, task_id: &str, depends_on: &[String]) -> TaskResult<()> {
        let tasks = self.list_tasks(None);
        let unknown: Vec<&str> = depends_on
            .iter()
            .filter(|id| !tasks.iter().any(|task| &task.id == *id))
            .map(String::as_str)
            .collect();
        if !unknown.is_empty() {
            return Err(TaskError::Edit(format!(
                "No such task: {}",
                unknown.join(", ")
            )));
        }
        let edges: Vec<(String, Vec<String>)> = tasks
            .iter()
            .map(|task| (task.id.clone(), task.depends_on.clone()))
            .collect();
        if makes_cycle(task_id, depends_on, &edges) {
            return Err(TaskError::Edit(
                "These dependencies would make a loop. A task cannot wait for itself".into(),
            ));
        }
        Ok(())
    }

    pub fn apply_event(&mut self, task_id: &str, raw: Option<&Value>) -> TaskResult<Applied> {
        let task = self.get_task(task_id)?;
        let mut input = apply_event_input(raw)?;
        let steps = self.steps_of(&task);
        // Read only when needed: an event that names its terminal and no stage, such as a bell.
        let payload = input
            .get("payload")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let by_terminal =
            payload.get("terminal").is_some_and(Value::is_string) && !payload.contains_key("stage");
        let pinned = if by_terminal {
            let events = self.list_events(task_id)?;
            pinned_terminals_of(events.iter().map(|stored| (stored.accepted, &stored.event)))
        } else {
            Vec::new()
        };
        input.insert("taskId".into(), json!(task_id));
        input.insert("at".into(), json!(self.timestamp()));
        let parsed = validate_task_event(&Value::Object(input))?;
        let position = position_of(&task);
        let event = complete_event(&position, parsed, &steps, &pinned);
        let applied = apply_task_event(&position, &event, &steps);
        let updated = if applied.accepted {
            let mut moved = move_task(&task, &applied.position, applied.moved, &event.at);
            moved.background_since = background_since_after(&task, &event, applied.position.state);
            moved.updated_at.clone_from(&event.at);
            moved
        } else {
            task
        };
        self.sync.store(
            updated.clone(),
            &[logged(event.clone(), applied.accepted)],
            applied.accepted,
        )?;
        self.publish(DaemonMessage::TaskUpdated {
            task: updated.clone(),
            event,
            accepted: applied.accepted,
            moved: Some(applied.moved),
        });
        Ok(Applied {
            task: updated,
            accepted: applied.accepted,
            moved: applied.moved,
        })
    }

    fn next_task_id(&self) -> String {
        // Ids the user chose with another prefix do not move the default numbers. A task whose
        // file is gone stays in memory, so its id counts too.
        let on_disk = list_task_ids(&self.data_dir()).unwrap_or_default();
        let highest = self
            .sync
            .tasks
            .keys()
            .chain(on_disk.iter())
            .filter_map(|id| id.strip_prefix("T-"))
            .filter_map(|number| number.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("T-{:04}", highest + 1)
    }
}

/// The parent's next stage waits for its subtasks, so a subtask waiting for the parent would wait
/// forever.
fn check_not_on_parent(parent_id: &str, depends_on: &[String]) -> TaskResult<()> {
    if depends_on.iter().any(|id| id == parent_id) {
        return Err(TaskError::Edit(
            "A subtask cannot depend on its parent".into(),
        ));
    }
    Ok(())
}

fn ids(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
