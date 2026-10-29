//! `stage-runner.ts`: moves tasks through their stages. Starts each stage when the task reaches
//! it, takes the report that a stage is done, checks the result and lets the gate decide
//! (docs/design-task-stages.md).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use natamux_core::events::apply::complete_event;
use natamux_core::events::derive::enters_branch;
use natamux_core::events::schema::validate_task_event;
use natamux_core::js::iso_from_millis;
use natamux_core::stages::lanes::terminal_lane;
use natamux_core::stages::terminal_lane::stage_of_terminal;
use natamux_core::tasks::dependencies::{open_subtasks, waiting_on};
use natamux_git::commit_push::delete_remote_ref;
use natamux_git::worktrees::tracks_path;
use natamux_proto::events::task_event::{RECORD_ONLY_EVENT_TYPES, TaskEvent};
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::TerminalLane;
use natamux_stages::actions::{ENDS_STAGE, lane_state};
use natamux_stages::ci::ci_branch_of;
use natamux_stages::files::{
    archive_stage_files, exclude_stage_dir, restore_stage_files, working_dir,
};
use natamux_stages::paths::{PathInput, pick_path};
use natamux_store::attachments::restore_attachments;
use natamux_store::task_files::STAGE_DIR;
use serde_json::{Value, json};

use crate::shared::event_bus::DaemonMessage;
use crate::stages::errors::{StageError, StageResult};
use crate::stages::run::{EventInput, StageRun, StageRunOptions, Work, key};
use crate::tasks::errors::TaskError;
use crate::tasks::helpers::position_of;
use crate::tasks::store::Applied;
use crate::terminals::limits::MAIN_TERMINAL;

// Logged as a workspace.warning event, which shows its message.
const TRACKED_STAGE_DIR: &str =
    "This repo tracks .natamux-task; stage files there may be pre-filled by the repo.";

/// The runner's handle: the daemon and the routes call it.
#[derive(Clone)]
pub struct StageRunner {
    pub run: Arc<StageRun>,
}

impl StageRunner {
    pub fn new(options: StageRunOptions) -> StageRunner {
        StageRunner {
            run: StageRun::new(options),
        }
    }

    pub fn start(&self) {
        let weak = Arc::downgrade(&self.run);
        let id = self.run.options.bus.subscribe(move |message| {
            if let Some(run) = weak.upgrade() {
                run.on_message(message);
            }
        });
        *self
            .run
            .subscription
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(id);
        self.run.resume_ci_checks();
    }

    pub fn stop(&self) {
        let id = self
            .run
            .subscription
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        if let Some(id) = id {
            self.run.options.bus.unsubscribe(id);
        }
    }

    /// Waits for the work the runner started in the background. For tests.
    pub fn settle(&self) {
        self.run.settle();
    }
}

impl StageRun {
    fn on_message(&self, message: &DaemonMessage) {
        match message {
            DaemonMessage::TaskDeleted { task_id } => {
                self.finish(task_id, false);
                // A deleted subtask no longer holds its parent.
                self.start_waiting();
            }
            DaemonMessage::TaskCreated { task } => self.on_created(task),
            DaemonMessage::TaskUpdated {
                task,
                event,
                accepted: true,
                moved,
            } => self.on_updated(task, event, *moved == Some(true)),
            _ => {}
        }
    }

    /// A new task whose first step reads its context starts it at once.
    fn on_created(&self, task: &Task) {
        let Ok(step) = self.step_of(task) else {
            return;
        };
        if step
            .stages()
            .iter()
            .any(|stage| stage.kind == StageKind::Context)
        {
            self.start_in_background(&task.id, &[TaskState::Draft, TaskState::Ready], None);
        }
    }

    fn on_updated(&self, task: &Task, event: &TaskEvent, moved: bool) {
        // Each piece of side work has its own error handling, so a failed one does not stop the
        // next stage.
        if ["stage.approved", "stage.passed", "stage.checked"].contains(&event.kind.as_str()) {
            self.archive(task);
        }
        self.delete_ci_branch(task);
        if event.kind == "workspace.created"
            && let Some(workspace) = &task.workspace
        {
            self.on_workspace_created(task, workspace);
        }
        if ENDS_STAGE.contains(&event.kind.as_str()) {
            self.end_stage_shell(task, event);
        }
        if event.kind == "stage.reopened" {
            self.end_left_shells(task);
        }
        if moved {
            self.start_in_background(&task.id, &[TaskState::Ready], Some(event.clone()));
        } else if event.kind == "agent.resumed" && event.source != "agent" {
            let (task_id, event) = (task.id.clone(), event.clone());
            self.background(move |run| run.resume_stage(&task_id, &event));
        }
        let ended = task.state.is_finished();
        if ended && !RECORD_ONLY_EVENT_TYPES.contains(&event.kind.as_str()) {
            self.finish(&task.id, true);
        }
        // A cancelled subtask no longer holds its parent.
        if ended || event.kind == "task.edited" {
            self.start_waiting();
        }
    }

    /// The CI branch only held snapshots for checks. A finished task that used CI deletes it once,
    /// in the project folder. Gone already, or offline: only logged.
    fn delete_ci_branch(&self, task: &Task) {
        let used_ci = task
            .ci_stages
            .as_ref()
            .is_some_and(|stages| !stages.is_empty());
        let (true, true, Some(branch)) = (task.state.is_finished(), used_ci, &task.branch) else {
            return;
        };
        if !self.state().ci_cleaned.insert(task.id.clone()) {
            return;
        }
        let (task_id, ref_name) = (task.id.clone(), ci_branch_of(branch));
        let folder = self.repo_path(task);
        self.background(move |_| {
            if let Err(error) = delete_remote_ref(&folder, &ref_name) {
                eprintln!("Could not delete the CI branch {ref_name} of {task_id}: {error}");
            }
        });
    }

    fn archive(&self, task: &Task) {
        if let Err(error) =
            archive_stage_files(self.data_dir(), &task.id, task.workspace.as_deref())
        {
            eprintln!("Could not archive the stage files of {}: {error}", task.id);
        }
    }

    fn on_workspace_created(&self, task: &Task, workspace: &str) {
        if let Err(error) = restore_stage_files(self.data_dir(), &task.id, Some(workspace)) {
            eprintln!("Could not restore the stage files of {}: {error}", task.id);
        }
        if let Err(error) =
            restore_attachments(self.data_dir(), &task.id, Some(Path::new(workspace)))
        {
            eprintln!("Could not restore the attachments of {}: {error}", task.id);
        }
        let (task_id, folder) = (task.id.clone(), PathBuf::from(workspace));
        self.background(move |run| {
            if let Err(error) = exclude_stage_dir(&folder) {
                eprintln!("Could not ignore the stage folder of {task_id}: {error}");
            }
            if tracks_path(&folder, STAGE_DIR) {
                let payload = json!({ "message": TRACKED_STAGE_DIR });
                let _ = run.record(&task_id, "workspace.warning", payload);
            }
        });
        if self.state().waiting_for_workspace.remove(&task.id) {
            self.start_in_background(&task.id, &[TaskState::Ready, TaskState::Draft], None);
        }
    }

    pub fn start_in_background(
        &self,
        task_id: &str,
        states: &[TaskState],
        cause: Option<TaskEvent>,
    ) {
        let work = Arc::new(Work::default());
        self.state()
            .start_work
            .insert(task_id.into(), Arc::clone(&work));
        let (task_id, states) = (task_id.to_string(), states.to_vec());
        self.background(move |run| {
            // Ends the work also when the start panics, so no action waits for it forever.
            let _done = WorkDone(Arc::clone(&work));
            if let Err(error) = run.start_lanes(&task_id, &states, cause.as_ref()) {
                eprintln!("Could not start the stages of {task_id}: {error}");
            }
            let mut state = run.state();
            if state
                .start_work
                .get(&task_id)
                .is_some_and(|now| Arc::ptr_eq(now, &work))
            {
                state.start_work.remove(&task_id);
            }
        });
    }

    /// Waits for the stages a task is starting now, also those a skip started after them.
    pub fn wait_for_start(&self, task_id: &str) {
        let mut work = self.state().start_work.get(task_id).cloned();
        while let Some(current) = work {
            current.wait();
            let next = self.state().start_work.get(task_id).cloned();
            work = next.filter(|next| !Arc::ptr_eq(next, &current));
        }
    }

    /// Starts the stages that waited for other tasks, once those are done.
    fn start_waiting(&self) {
        if self.state().waiting_for_tasks.is_empty() {
            return;
        }
        let all = self.all_tasks();
        let waiting: Vec<String> = self.state().waiting_for_tasks.iter().cloned().collect();
        for id in waiting {
            let task = all.iter().find(|candidate| candidate.id == id);
            let free = task.is_none_or(|task| {
                waiting_on(task, &all).is_empty() && open_subtasks(task, &all).is_empty()
            });
            if free {
                self.state().waiting_for_tasks.remove(&id);
                if task.is_some() {
                    self.start_in_background(&id, &[TaskState::Ready, TaskState::Draft], None);
                }
            }
        }
    }

    /// Starts the stages of the task's step that are in one of `states`.
    pub fn start_lanes(
        &self,
        task_id: &str,
        states: &[TaskState],
        cause: Option<&TaskEvent>,
    ) -> StageResult<()> {
        let forward = cause.is_some_and(|cause| ENDS_STAGE.contains(&cause.kind.as_str()));
        // The skips ended the step: the message of that move starts the next one.
        if forward && !self.marked_lanes(task_id)?.is_empty() && self.skip_marked(task_id)? {
            return Ok(());
        }
        let task = self.task(task_id)?;
        let step = self.step_of(&task)?;
        for stage in step.stages() {
            if states.contains(&lane_state(&task, &step, &stage.id)) {
                self.start_stage(&task.id, stage, cause)?;
            }
        }
        Ok(())
    }

    /// The stages of the current step that the user marked to skip, and that are not done.
    fn marked_lanes(&self, task_id: &str) -> StageResult<Vec<Stage>> {
        let task = self.task(task_id)?;
        let marked = task.skip_stages.clone().unwrap_or_default();
        let step = self.step_of(&task)?;
        Ok(step
            .stages()
            .into_iter()
            .filter(|stage| {
                marked.contains(&stage.id) && lane_state(&task, &step, &stage.id) != TaskState::Done
            })
            .cloned()
            .collect())
    }

    /// Skips the marked stages of the current step. True when that ended the step.
    fn skip_marked(&self, task_id: &str) -> StageResult<bool> {
        let before = self.task(task_id)?;
        for stage in self.marked_lanes(task_id)? {
            let input = EventInput::new(
                "system",
                "stage.skipped",
                json!({ "stage": stage.id, "planned": true }),
            );
            self.apply_ending(task_id, input)?;
        }
        let after = self.task(task_id)?;
        Ok(after.stage != before.stage || after.state == TaskState::Done)
    }

    pub fn start_stage(
        &self,
        task_id: &str,
        stage: &Stage,
        cause: Option<&TaskEvent>,
    ) -> StageResult<()> {
        let starting = key(task_id, &stage.id);
        if !self.state().starting.insert(starting.clone()) {
            return Ok(());
        }
        let result = self.start_stage_once(task_id, stage, cause);
        self.state().starting.remove(&starting);
        result
    }

    fn start_stage_once(
        &self,
        task_id: &str,
        stage: &Stage,
        cause: Option<&TaskEvent>,
    ) -> StageResult<()> {
        let task = self.task(task_id)?;
        match stage.kind {
            StageKind::Context => self.start_context(&task, stage),
            StageKind::Agent => {
                let resumed = cause.is_some_and(|cause| cause.kind == "agent.resumed");
                let prompt_for = self.prompt_of(&task, stage, cause);
                self.start_agent(&task, stage, prompt_for, resumed)
            }
            StageKind::Testing => match &stage.command {
                None => {
                    let prompt_for = self.prompt_of(&task, stage, None);
                    self.start_agent(&task, stage, prompt_for, false)
                }
                Some(command) => self.start_command(&task, stage, command),
            },
            // Review, manual and release stages wait for a person or a merge.
            _ => Ok(()),
        }
    }

    /// A move into a branch takes the first path whose condition holds. The conditions are
    /// checked here, before the move, and the event keeps the choice, so a replay of the log never
    /// runs them.
    pub fn with_path(&self, task_id: &str, mut input: EventInput) -> StageResult<EventInput> {
        let task = self.task(task_id)?;
        let steps = self.steps(&task)?;
        let position = position_of(&task);
        let mut probe = input.to_value();
        if let Some(map) = probe.as_object_mut() {
            map.insert("taskId".into(), json!(task_id));
            map.insert("at".into(), json!(iso_from_millis(self.now())));
        }
        let event = validate_task_event(&probe).map_err(TaskError::Invalid)?;
        let event = complete_event(&position, event, &steps, &Vec::new());
        let Some(branch) = enters_branch(&position, &event, &steps) else {
            input.payload.shift_remove("path");
            input.payload.shift_remove("because");
            return Ok(input);
        };
        // The conditions and their order come from the settings now, as names and gates do. A
        // path whose stages are all off now still runs for a task that has it, so it is checked.
        let current = self
            .settings(&task)
            .stages
            .into_iter()
            .find(|step| step.id() == branch.id);
        let checked = match current {
            Some(Step::Branch(current)) => current,
            _ => branch.clone(),
        };
        self.check_path_trust(&task, &checked)?;
        let folder = working_dir(self.data_dir(), &task.id, task.workspace.as_deref());
        let cwd = task
            .workspace
            .as_ref()
            .map_or_else(|| folder.clone(), PathBuf::from);
        let paths = branch.paths.iter().map(|path| path.id.clone()).collect();
        let picked = pick_path(
            &checked,
            &PathInput { folder, cwd, paths },
            &self.options.run_command,
        );
        // Only the daemon picks the path. The stage stays the one the event was for, though the
        // conditions took a while.
        let mut payload = event.payload;
        payload.shift_remove("path");
        payload.shift_remove("because");
        if let Some(path) = picked.path {
            payload.insert("path".into(), json!(path));
        }
        payload.insert("because".into(), json!(picked.because));
        input.payload = payload;
        Ok(input)
    }

    /// An event that may end a stage, with the path it takes at a branch it enters.
    pub fn apply_ending(&self, task_id: &str, input: EventInput) -> StageResult<Applied> {
        let input = self.with_path(task_id, input)?;
        self.apply(task_id, &input)
    }

    /// The stage a hook from this terminal is about, and its state. None for a shell that no
    /// stage uses, where the user started an agent by hand. Part 4's hook service calls it.
    pub fn lane_for(
        &self,
        task: &Task,
        terminal: Option<&str>,
    ) -> StageResult<Option<(String, TaskState)>> {
        let step = self.step_of(task)?;
        let steps = self.steps(task)?;
        let pinned = self.pinned(&task.id);
        let stage = stage_of_terminal(&steps, &step, terminal.unwrap_or(MAIN_TERMINAL), &pinned);
        Ok(stage.map(|stage| {
            let state = lane_state(task, &step, &stage.id);
            (stage.id, state)
        }))
    }

    /// The stage an agent in this terminal works on, and its own state, for the dashboard.
    pub fn lane_of_terminal(&self, task: &Task, terminal: &str) -> StageResult<TerminalLane> {
        let steps = self.steps(task)?;
        let pinned = self.pinned(&task.id);
        Ok(terminal_lane(&position_of(task), &steps, terminal, &pinned))
    }

    /// True once for the turn end that follows a report from this terminal. A shell that waits
    /// for this turn end to close closes now.
    pub fn take_report(&self, task_id: &str, terminal: &str) -> bool {
        let taken = self
            .state()
            .reported
            .remove(&key(task_id, terminal))
            .is_some();
        self.close_pending_shell(task_id, terminal);
        taken
    }

    /// The stage reported from this terminal whose turn end has not come yet. Its usage, up to
    /// that turn end, counts for that stage.
    pub fn report_of(&self, task_id: &str, terminal: &str) -> Option<String> {
        self.state().reported.get(&key(task_id, terminal)).cloned()
    }

    /// A task made with no agent: its stages that need one stay ready, and no agent starts in its
    /// shell until a person starts one.
    pub fn leave_to_person(&self, task_id: &str) -> StageResult<()> {
        Ok(self.options.tasks.set_without_agent(task_id, true)?)
    }
}

/// Finishes a `Work` when dropped.
struct WorkDone(Arc<Work>);

impl Drop for WorkDone {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// The error text of a failed piece of work, for a stage that fails.
pub fn error_text(error: &StageError) -> String {
    error.to_string()
}

/// A JSON string field of an event's payload.
pub fn payload_text<'a>(event: &'a TaskEvent, name: &str) -> Option<&'a str> {
    event.payload.get(name).and_then(Value::as_str)
}
