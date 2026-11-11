//! The task service (`task-service.ts`): every call runs under one lock, so it is whole, as a call
//! on Node's one thread is; its messages go out after the lock is let go. The file writes of a
//! hook's changes run after that lock, in each task's order (`with_later`).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::{PullRequestStatus, Task, TaskState};
use natamux_store::attachments::AttachmentFile;
use natamux_store::task_index::TaskIndex;
use natamux_store::task_sync::TaskSync;
use natamux_store::task_writes::Writes;
use natamux_store::watch::{WATCH_DELAY, watch_task_files};
use serde_json::Value;

use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::jobs::{Clock, system_clock};
use crate::tasks::errors::{TaskError, TaskResult};
use crate::tasks::helpers::legacy_pipeline;
use crate::tasks::records::{ActivityOptions, WorkspaceChange, activity_due};
use crate::tasks::store::{Applied, Catalog, SettingsOf, Store};

pub struct TaskServiceOptions {
    pub data_dir: PathBuf,
    pub index: TaskIndex,
    pub bus: Arc<EventBus>,
    /// The time of new events, in milliseconds. Default: the system clock.
    pub now: Option<Clock>,
    /// The stages of a repository. Default: the legacy pipeline.
    pub settings_of: Option<SettingsOf>,
    /// In milliseconds, for the checks of the task files. Default: the system clock.
    pub clock: Option<Clock>,
}

#[derive(Clone)]
pub struct TaskService {
    store: Arc<Mutex<Store>>,
    bus: Arc<EventBus>,
    now: Clock,
    settings_of: SettingsOf,
}

impl TaskService {
    pub fn new(options: TaskServiceOptions) -> TaskService {
        let bus = options.bus;
        let changed = Arc::clone(&bus);
        let sync = TaskSync::new(
            &options.data_dir,
            options.index,
            options.clock.unwrap_or_else(system_clock),
            Box::new(move |task: &Task| {
                changed.enqueue(DaemonMessage::TaskChanged { task: task.clone() });
            }),
        );
        let now = options.now.unwrap_or_else(system_clock);
        let settings_of: SettingsOf = options
            .settings_of
            .unwrap_or_else(|| Arc::new(|_| legacy_pipeline()));
        let store = Store {
            sync,
            bus: Arc::clone(&bus),
            settings_of: Arc::clone(&settings_of),
            now: Arc::clone(&now),
        };
        TaskService {
            store: Arc::new(Mutex::new(store)),
            bus,
            now,
            settings_of,
        }
    }

    fn lock(&self) -> TaskResult<MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| TaskError::Failed("The task store failed earlier".into()))
    }

    /// Runs `work` under the lock, then sends the messages it queued.
    pub fn with<T>(&self, work: impl FnOnce(&mut Store) -> TaskResult<T>) -> TaskResult<T> {
        self.run(false, work)
    }

    /// As `with`, but the disk work of the change runs after the lock is let go, so changes of
    /// other tasks need not wait for it. Only for calls whose store is their last step.
    fn with_later<T>(&self, work: impl FnOnce(&mut Store) -> TaskResult<T>) -> TaskResult<T> {
        self.run(true, work)
    }

    fn run<T>(
        &self,
        deferred: bool,
        work: impl FnOnce(&mut Store) -> TaskResult<T>,
    ) -> TaskResult<T> {
        let (result, writes) = {
            let mut store = self.lock()?;
            if deferred {
                store.sync.defer_writes();
            }
            let result = work(&mut store);
            (result, store.sync.take_writes())
        };
        let written = writes.map_or(Ok(()), |writes| self.finish(writes));
        self.bus.flush();
        let value = result?;
        written?;
        Ok(value)
    }

    /// Runs the writes left for later. A task whose write failed goes back to what is stored.
    fn finish(&self, writes: Writes) -> TaskResult<()> {
        let ran = writes.run();
        if !ran.failed.is_empty() {
            let mut store = self.lock()?;
            for task_id in &ran.failed {
                store.sync.restore(task_id);
            }
        }
        ran.result.map_err(TaskError::from)
    }

    /// Loads every task into memory and watches the task files, until `close`.
    pub fn init(&self) -> TaskResult<()> {
        let data_dir = self.with(|store| {
            store.sync.init()?;
            Ok(store.data_dir())
        })?;
        let weak: Weak<Mutex<Store>> = Arc::downgrade(&self.store);
        let bus = Arc::clone(&self.bus);
        let watch = watch_task_files(&data_dir, WATCH_DELAY, move |ids: BTreeSet<String>| {
            let Some(store) = weak.upgrade() else {
                return;
            };
            if let Ok(mut store) = store.lock() {
                for id in ids {
                    store.sync.try_reload(&id);
                }
            }
            bus.flush();
        });
        self.with(|store| {
            store.sync.set_watch(watch);
            Ok(())
        })
    }

    /// Stops watching the task files.
    pub fn close(&self) {
        let _ = self.with(|store| {
            store.sync.close();
            Ok(())
        });
    }

    pub fn create_task(
        &self,
        raw: Option<&Value>,
        attachments: &[AttachmentFile],
        split_id: bool,
    ) -> TaskResult<Task> {
        self.with(|store| store.create_task(raw, attachments, split_id))
    }

    pub fn get_task(&self, task_id: &str) -> TaskResult<Task> {
        self.with(|store| store.get_task(task_id))
    }

    pub fn list_tasks(&self, state: Option<TaskState>) -> TaskResult<Vec<Task>> {
        self.with(|store| Ok(store.list_tasks(state)))
    }

    /// The task a hook is for, in one lock take: the one the header names, else the one whose
    /// workspace `holds` accepts. None when there is none.
    pub fn task_for_hook(
        &self,
        header: Option<&str>,
        holds: Option<&dyn Fn(&str) -> bool>,
    ) -> TaskResult<Option<Task>> {
        self.with(|store| {
            if let Some(header) = header {
                match store.get_task(header) {
                    Ok(task) => return Ok(Some(task)),
                    Err(TaskError::NotFound(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            let Some(holds) = holds else {
                return Ok(None);
            };
            match store.id_in_workspace(holds) {
                Some(id) => match store.get_task(&id) {
                    Ok(task) => Ok(Some(task)),
                    Err(TaskError::NotFound(_)) => Ok(None),
                    Err(error) => Err(error),
                },
                None => Ok(None),
            }
        })
    }

    pub fn has_task(&self, task_id: &str) -> TaskResult<bool> {
        self.with(|store| store.has_task(task_id))
    }

    /// The id of the task `list_tasks` lists first among those whose workspace `holds` accepts.
    pub fn id_in_workspace(&self, holds: impl Fn(&str) -> bool) -> TaskResult<Option<String>> {
        self.with(|store| Ok(store.id_in_workspace(holds)))
    }

    pub fn workspace_list(&self) -> TaskResult<Vec<crate::tasks::store::WorkspaceRef>> {
        self.with(|store| Ok(store.workspace_list()))
    }

    pub fn list_events(&self, task_id: &str) -> TaskResult<Vec<StoredEvent>> {
        self.with(|store| store.list_events(task_id))
    }

    pub fn update_task(&self, task_id: &str, raw: Option<&Value>) -> TaskResult<Task> {
        self.with(|store| store.update_task(task_id, raw))
    }

    pub fn delete_task(&self, task_id: &str) -> TaskResult<Vec<Task>> {
        self.with(|store| store.delete_task(task_id))
    }

    pub fn apply_event(&self, task_id: &str, raw: Option<&Value>) -> TaskResult<Applied> {
        self.with_later(|store| store.apply_event(task_id, raw))
    }

    /// The stages of a repository, read with no lock, for `apply_event_in`.
    pub fn catalog(&self, repo: Option<&str>) -> Catalog {
        (repo.map(str::to_string), (self.settings_of)(repo))
    }

    /// `apply_event` for a caller that holds a copy of the task, such as a hook: `catalog`, the
    /// stages of its repository, is read before the lock that the other tasks' changes wait for.
    pub fn apply_event_in(
        &self,
        task_id: &str,
        catalog: &Catalog,
        raw: Option<&Value>,
    ) -> TaskResult<Applied> {
        self.with_later(|store| store.apply_event_on(task_id, raw, Some(catalog)))
    }

    pub fn steps_of(&self, task: &Task) -> TaskResult<Vec<natamux_proto::stages::schema::Step>> {
        self.with(|store| Ok(store.steps_of(task)))
    }

    pub fn record_workspace(&self, task_id: &str, change: WorkspaceChange) -> TaskResult<Task> {
        self.with(|store| store.record_workspace(task_id, change))
    }

    pub fn record_main_checkout(
        &self,
        task_id: &str,
        workspace: String,
        branch: Option<String>,
    ) -> TaskResult<Task> {
        self.with(|store| store.record_main_checkout(task_id, workspace, branch))
    }

    pub fn record_workspace_failure(&self, task_id: &str, message: &str) -> TaskResult<Task> {
        self.with(|store| store.record_workspace_failure(task_id, message))
    }

    pub fn record_branch(
        &self,
        task_id: &str,
        from: Option<String>,
        to: String,
    ) -> TaskResult<Task> {
        self.with(|store| store.record_branch(task_id, from, to))
    }

    pub fn stop_agents(&self, task_id: &str) -> TaskResult<Task> {
        self.with(|store| store.stop_agents(task_id))
    }

    pub fn record_git(&self, task_id: &str, kind: &str, payload: Value) -> TaskResult<Task> {
        self.with(|store| store.record_git(task_id, kind, payload))
    }

    pub fn record_review_comment(&self, task_id: &str, text: &str) -> TaskResult<Task> {
        self.with(|store| store.record_review_comment(task_id, text))
    }

    pub fn record_review_comments(&self, task_id: &str, ids: &[String]) -> TaskResult<Task> {
        self.with(|store| store.record_review_comments(task_id, ids))
    }

    pub fn record_agent_launch(
        &self,
        task_id: &str,
        agent: &str,
        program: Option<&str>,
        stage: Option<&str>,
        restored_in: Option<&str>,
        launch: (&str, Option<&str>),
    ) -> TaskResult<Task> {
        self.with(|store| {
            store.record_agent_launch(task_id, agent, program, stage, restored_in, launch)
        })
    }

    pub fn record_pull_request_status(
        &self,
        task_id: &str,
        status: PullRequestStatus,
    ) -> TaskResult<Task> {
        self.with(|store| store.record_pull_request_status(task_id, status))
    }

    pub fn link_issue(&self, task_id: &str, raw: &Value, source: &str) -> TaskResult<Task> {
        self.with(|store| store.link_issue(task_id, raw, source))
    }

    pub fn set_workflow(&self, task_id: &str, stage_id: &str, on: bool) -> TaskResult<Task> {
        self.with(|store| store.set_workflow(task_id, stage_id, on))
    }

    pub fn set_ci(&self, task_id: &str, stage_id: &str, on: bool) -> TaskResult<Task> {
        self.with(|store| store.set_ci(task_id, stage_id, on))
    }

    pub fn set_skip(&self, task_id: &str, stage_id: &str, on: bool) -> TaskResult<Task> {
        self.with(|store| store.set_skip(task_id, stage_id, on))
    }

    pub fn set_without_agent(&self, task_id: &str, on: bool) -> TaskResult<()> {
        self.with(|store| store.set_without_agent(task_id, on))
    }

    pub fn record_event(
        &self,
        task_id: &str,
        kind: &str,
        payload: Value,
        source: &str,
    ) -> TaskResult<Task> {
        self.with(|store| store.record_event(task_id, kind, payload, source))
    }

    pub fn record_activity(&self, task_id: &str, options: ActivityOptions) -> TaskResult<bool> {
        self.with_later(|store| store.record_activity(task_id, options))
    }

    /// Whether `record_activity` would change `task` now, judged on this copy of it, with no
    /// lock: the common, throttled sign of life then costs no lock take at all.
    pub fn activity_due(&self, task: &Task, options: &ActivityOptions) -> bool {
        activity_due(task, (self.now)(), options)
    }

    /// The data folder of the tasks.
    pub fn data_dir(&self) -> TaskResult<PathBuf> {
        self.with(|store| Ok(store.data_dir()))
    }
}

/// A done or cancelled task needs its shells no more. Neither does a task that is gone.
pub fn is_task_ended(tasks: &TaskService, task_id: &str) -> bool {
    tasks
        .get_task(task_id)
        .map_or(true, |task| task.state.is_finished())
}
