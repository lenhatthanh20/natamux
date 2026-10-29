//! `agent-hook-service.ts`: an agent's hook moves its task (needs input, asked, finished, failed,
//! resumed), shows what the agent does, names its session and its usage file. One hook is handled
//! whole, as on Node's one thread: `handle` runs under one lock.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use natamux_agents::hooks::{NormalizedHook, Signal, find_adapter, plan_signal_events};
use natamux_core::tasks::inputs::task_id_param;
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::{Map, Value, json};

use super::errors::HookError;
use super::hook_activity::Activities;
use super::hook_usage::{TurnStage, UsageStages};
use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::stages::runner::StageRunner;
use crate::tasks::errors::TaskError;
use crate::tasks::records::ActivityOptions;
use crate::tasks::service::TaskService;
use crate::terminals::limits::MAIN_TERMINAL;

#[derive(Debug, PartialEq)]
pub struct HookResult {
    pub task_id: Option<String>,
    pub applied: Vec<&'static str>,
}

impl HookResult {
    fn none() -> HookResult {
        HookResult {
            task_id: None,
            applied: Vec::new(),
        }
    }
}

/// The stage a terminal works on, and the state of its lane.
pub type Lane = (String, TaskState);

/// What the hook service needs to know about stages: which stage a terminal works on (None for
/// none), and whether a turn end in a terminal follows a report (see `StageRun`).
pub trait StageLookup: Send + Sync {
    fn lane_for(&self, task: &Task, terminal: Option<&str>) -> Result<Option<Lane>, String>;
    fn take_report(&self, task_id: &str, terminal: &str) -> bool;
    /// The stage reported from this terminal whose turn end has not come yet, without taking it.
    fn report_of(&self, _task_id: &str, _terminal: &str) -> Option<String> {
        None
    }
}

impl StageLookup for StageRunner {
    fn lane_for(&self, task: &Task, terminal: Option<&str>) -> Result<Option<Lane>, String> {
        self.run
            .lane_for(task, terminal)
            .map_err(|error| error.to_string())
    }

    fn take_report(&self, task_id: &str, terminal: &str) -> bool {
        self.run.take_report(task_id, terminal)
    }

    fn report_of(&self, task_id: &str, terminal: &str) -> Option<String> {
        self.run.report_of(task_id, terminal)
    }
}

/// No stages: the task's own stage and state, and no report.
pub struct NoStages;

impl StageLookup for NoStages {
    fn lane_for(&self, task: &Task, _terminal: Option<&str>) -> Result<Option<Lane>, String> {
        Ok(Some((task.stage.clone(), task.state)))
    }

    fn take_report(&self, _task_id: &str, _terminal: &str) -> bool {
        false
    }
}

/// The state of an agent in a shell that no stage uses. A turn end waits for the user, as it does
/// for the task's own agent.
fn shell_state_of(signal: Signal) -> TaskState {
    match signal {
        Signal::Prompt | Signal::Activity | Signal::TurnPaused => TaskState::Running,
        Signal::Asking | Signal::TurnEnded => TaskState::NeedsInput,
        Signal::Failed => TaskState::Failed,
    }
}

/// `/^(pre|before)/i`
fn is_before_tool(name: &str) -> bool {
    let starts = |word: &str| {
        name.get(..word.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(word))
    };
    starts("pre") || starts("before")
}

/// Only a tool that ran, or the end of a turn, can have changed files. A tool that is about to run
/// (PreToolUse, beforeShellExecution, BeforeTool) changed nothing yet.
fn changes_files(hook: &NormalizedHook) -> bool {
    match hook.signal {
        Signal::Activity => hook
            .details
            .get("hook")
            .and_then(Value::as_str)
            .is_none_or(|name| !is_before_tool(name)),
        Signal::TurnEnded | Signal::TurnPaused | Signal::Failed => true,
        Signal::Prompt | Signal::Asking => false,
    }
}

/// The signals after which the session's file holds the usage of a whole model call.
fn reads_usage(signal: Signal) -> bool {
    matches!(
        signal,
        Signal::TurnEnded | Signal::TurnPaused | Signal::Failed | Signal::Asking
    )
}

/// The parts of a path, as Node's `path.relative` reads them: `.` and `..` resolved, and on
/// Windows both slashes and any case.
fn parts_of(path: &str) -> Vec<String> {
    let text = if cfg!(windows) {
        path.replace('\\', "/").to_lowercase()
    } else {
        path.to_string()
    };
    let mut parts: Vec<String> = Vec::new();
    for part in text.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    parts
}

fn is_inside(folder: &str, path: &str) -> bool {
    let folder = parts_of(folder);
    !folder.is_empty() && parts_of(path).starts_with(&folder)
}

pub type SessionSeen = Arc<dyn Fn(&str, &str, &str, &str) + Send + Sync>;
pub type UsageSeen = Arc<dyn Fn(&str, Option<&str>, &str, &str) + Send + Sync>;
pub type WorkspaceChanged = Arc<dyn Fn(&str) + Send + Sync>;
pub type SignalSeen = Arc<dyn Fn(&str, &str, Signal) + Send + Sync>;

pub struct HookOptions {
    /// Dashboards hear of each change of the activity, and of the state of an agent in a shell.
    pub bus: Option<Arc<EventBus>>,
    /// Called after a hook that may have changed files in the task's workspace.
    pub workspace_changed: Option<WorkspaceChanged>,
    /// Called with each session a hook names from a Natamux shell: task, terminal, agent,
    /// session. The shell can then come back on it.
    pub session_seen: Option<SessionSeen>,
    /// Called with the signal of each hook of a task: task, terminal, signal. A turn end that
    /// reports a stage counts too.
    pub signal_seen: Option<SignalSeen>,
    /// Called after a turn with the session's usage file and the stage it counts for (None:
    /// none): task, stage, agent, file.
    pub usage_seen: Option<UsageSeen>,
    /// Where Copilot keeps its sessions.
    pub home: PathBuf,
}

#[derive(Default)]
struct State {
    activities: Activities,
    /// The state of each agent in a shell that no stage uses, by task id, then terminal.
    shell_states: HashMap<String, HashMap<String, TaskState>>,
    /// "<task>/<terminal>" of the shells whose state the last read of the task's shells dropped,
    /// as no agent runs there for it (see `forget_shells_except`). No dashboard shows their
    /// state, so a hook there sends no shells.changed: each one would only read the shells, and
    /// drop it, again.
    hidden_shells: HashSet<String>,
    /// The stage and session file of each shell's last hook, for its usage.
    usage: UsageStages,
}

pub struct HookService {
    tasks: TaskService,
    stages: Arc<dyn StageLookup>,
    options: HookOptions,
    /// One hook at a time for each task, so two hooks of a task never interleave while hooks of
    /// other tasks run on. All the state a hook writes is kept by task id.
    serial: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    state: Mutex<State>,
}

impl HookService {
    pub fn new(tasks: TaskService, stages: Arc<dyn StageLookup>, options: HookOptions) -> Self {
        HookService {
            tasks,
            stages,
            options,
            serial: Mutex::new(HashMap::new()),
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The lock of a task. A lock that no hook holds or waits for is dropped from the map here, so
    /// deleted tasks leave nothing behind.
    fn task_lock(&self, task_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.serial.lock().unwrap_or_else(PoisonError::into_inner);
        locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        Arc::clone(locks.entry(task_id.to_string()).or_default())
    }

    fn publish(&self, message: DaemonMessage) {
        if let Some(bus) = &self.options.bus {
            bus.publish(message);
        }
    }

    fn publish_activity(&self, task_id: &str, activity: Option<Map<String, Value>>) {
        if let Some(activity) = activity {
            self.publish(DaemonMessage::ActivityChanged {
                task_id: task_id.to_string(),
                activity,
            });
        }
    }

    /// The task comes from the header when it names one, otherwise from the workspace that holds
    /// `cwd`.
    pub fn find_task(
        &self,
        cwd: Option<&str>,
        task_id_header: Option<&str>,
    ) -> Result<Option<Task>, TaskError> {
        if let Some(header) = task_id_header
            && task_id_param(header).is_ok()
        {
            match self.tasks.get_task(header) {
                Ok(task) => return Ok(Some(task)),
                Err(TaskError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let Some(cwd) = cwd else {
            return Ok(None);
        };
        self.tasks
            .task_in_workspace(|workspace| is_inside(workspace, cwd))
    }

    /// What the agent of each task does now, from its last hook: `{ task: { terminal: line } }`.
    pub fn activity(&self) -> Map<String, Value> {
        self.state().activities.all()
    }

    pub fn shell_state(&self, task_id: &str, terminal: &str) -> Option<TaskState> {
        self.state()
            .shell_states
            .get(task_id)
            .and_then(|states| states.get(terminal))
            .copied()
    }

    /// Called with the shells where an agent runs now. An agent started later in another shell
    /// must not show the state of one that ended there.
    pub fn forget_shells_except(&self, task_id: &str, running: &[String]) {
        let mut state = self.state();
        let State {
            shell_states,
            hidden_shells,
            ..
        } = &mut *state;
        if let Some(states) = shell_states.get_mut(task_id) {
            states.retain(|terminal, _| {
                let keep = running.contains(terminal);
                if !keep {
                    hidden_shells.insert(format!("{task_id}/{terminal}"));
                }
                keep
            });
            if states.is_empty() {
                shell_states.remove(task_id);
            }
        }
        for terminal in running {
            hidden_shells.remove(&format!("{task_id}/{terminal}"));
        }
    }

    /// `UnknownAgent` for an agent without an adapter, and `Invalid` when the input is not a hook
    /// of that agent.
    pub fn handle(
        &self,
        agent_id: &str,
        raw: &Value,
        task_id_header: Option<&str>,
        terminal: Option<&str>,
    ) -> Result<HookResult, HookError> {
        self.handle_in(agent_id, raw, task_id_header, terminal, true)
    }

    /// A hook sent with a task's agent token: for that task only, never one its folder names.
    pub fn handle_own(
        &self,
        agent_id: &str,
        raw: &Value,
        own: &str,
        terminal: Option<&str>,
    ) -> Result<HookResult, HookError> {
        self.handle_in(agent_id, raw, Some(own), terminal, false)
    }

    fn handle_in(
        &self,
        agent_id: &str,
        raw: &Value,
        task_id_header: Option<&str>,
        terminal: Option<&str>,
        by_cwd: bool,
    ) -> Result<HookResult, HookError> {
        let adapter =
            find_adapter(agent_id).ok_or_else(|| HookError::UnknownAgent(agent_id.to_string()))?;
        let Some(hook) = adapter
            .normalize(raw, &self.options.home)
            .map_err(HookError::Invalid)?
        else {
            return Ok(HookResult::none());
        };
        let cwd = hook.cwd.as_deref().filter(|_| by_cwd);
        let Some(found) = self.find_task(cwd, task_id_header)? else {
            return Ok(HookResult::none());
        };
        let lock = self.task_lock(&found.id);
        let _one_at_a_time = lock.lock().unwrap_or_else(PoisonError::into_inner);
        // The hook before this one may have changed the task while this one waited.
        let task = match self.tasks.get_task(&found.id) {
            Ok(task) => task,
            Err(TaskError::NotFound(_)) => return Ok(HookResult::none()),
            Err(error) => return Err(error.into()),
        };
        // Natamux terminals run in the task's workspace only. Without one, the hook comes from an
        // agent that the removal of the workspace ended.
        if task.workspace.is_none() && terminal.is_some() {
            let gone = self.state().activities.forget_task(&task.id);
            self.publish_activity(&task.id, gone);
            return Ok(HookResult::none());
        }
        let shell = terminal.unwrap_or(MAIN_TERMINAL);
        let noted = self.state().activities.note(&task.id, shell, &hook);
        self.publish_activity(&task.id, noted);
        let session = hook.details.get("sessionId").and_then(Value::as_str);
        if let (Some(terminal), Some(session), Some(seen)) =
            (terminal, session, &self.options.session_seen)
            && !session.is_empty()
        {
            seen(&task.id, terminal, adapter.id(), session);
        }
        if changes_files(&hook)
            && let Some(changed) = &self.options.workspace_changed
        {
            changed(&task.id);
        }
        if let Some(seen) = &self.options.signal_seen {
            seen(&task.id, shell, hook.signal);
        }
        self.after_noted(&task, &hook, adapter.id(), terminal, shell)
    }

    /// The stage side of `handle`: the lane, the report, the usage, then the task's events or the
    /// shell's own state.
    fn after_noted(
        &self,
        task: &Task,
        hook: &NormalizedHook,
        agent: &str,
        terminal: Option<&str>,
        shell: &str,
    ) -> Result<HookResult, HookError> {
        // A finished task stays as it is, even with lanes left running: an agent reopened in it
        // has a state of its own, as one on no stage.
        let lane = if task.state.is_finished() {
            None
        } else {
            self.stages
                .lane_for(task, terminal)
                .map_err(HookError::Failed)?
        };
        let pending = self.stages.report_of(&task.id, shell);
        // The turn in which the agent ran `natamux done`, or sent the task back, ends as planned,
        // not "stopped early", even when background work is still running.
        let reported = matches!(hook.signal, Signal::TurnEnded | Signal::TurnPaused)
            && self.stages.take_report(&task.id, shell);
        let usage = TurnStage {
            lane: lane.as_ref(),
            pending,
            reported,
        };
        self.note_usage(&task.id, shell, agent, hook, usage);
        let done = |applied| HookResult {
            task_id: Some(task.id.clone()),
            applied,
        };
        if reported {
            let forgotten = self.state().activities.forget(&task.id, shell);
            self.publish_activity(&task.id, forgotten);
            return Ok(done(Vec::new()));
        }
        let Some((stage, state)) = lane else {
            // An agent on no stage of the step: one the user started by hand, or a reviewer that
            // sent the task back. It has a state of its own and leaves the task alone.
            self.set_shell_state(&task.id, shell, shell_state_of(hook.signal));
            return Ok(done(Vec::new()));
        };
        // The task came back to the stage of this shell: a state from the time in between is old.
        if let Some(states) = self.state().shell_states.get_mut(&task.id) {
            states.remove(shell);
        }
        let mut applied = Vec::new();
        for planned in plan_signal_events(state, hook.signal, &hook.details) {
            let mut payload = planned.payload.clone();
            payload.insert("stage".into(), Value::String(stage.clone()));
            let input = json!({
                "source": "agent",
                "type": planned.kind,
                "payload": Value::Object(payload),
            });
            self.tasks.apply_event(&task.id, Some(&input))?;
            applied.push(planned.kind);
        }
        // A hook that changes nothing still shows that a running agent is alive.
        if applied.is_empty() {
            self.tasks
                .record_activity(&task.id, ActivityOptions::default())?;
        }
        Ok(done(applied))
    }

    fn set_shell_state(&self, task_id: &str, shell: &str, new: TaskState) {
        let tell = {
            let mut state = self.state();
            let State {
                shell_states,
                hidden_shells,
                ..
            } = &mut *state;
            let states = shell_states.entry(task_id.to_string()).or_default();
            if states.get(shell) == Some(&new) {
                false
            } else {
                states.insert(shell.to_string(), new);
                // The dashboards show this state with the task's shells.
                !hidden_shells.contains(&format!("{task_id}/{shell}"))
            }
        };
        if tell {
            self.publish(DaemonMessage::ShellsChanged {
                task_id: task_id.to_string(),
            });
        }
    }

    fn note_usage(
        &self,
        task_id: &str,
        terminal: &str,
        agent: &str,
        hook: &NormalizedHook,
        turn: TurnStage<'_>,
    ) {
        let shell = format!("{task_id}/{terminal}");
        let file = hook.transcript.as_deref().map(|file| (agent, file));
        let stage = self.state().usage.note(&shell, turn, file);
        if let (Some(transcript), Some(seen)) = (&hook.transcript, &self.options.usage_seen)
            && reads_usage(hook.signal)
        {
            seen(task_id, stage.as_deref(), agent, transcript);
        }
    }

    /// The process of a shell ended. No hook may follow what its agent wrote after the last
    /// read, such as a turn cut off when the task finished, so its file is read once more. The
    /// next agent in this shell starts with no stage and no file.
    pub fn shell_ended(&self, task_id: &str, terminal: &str) {
        let (stage, file) = self.state().usage.take(&format!("{task_id}/{terminal}"));
        if let (Some((agent, transcript)), Some(seen)) = (file, &self.options.usage_seen) {
            seen(task_id, stage.as_deref(), &agent, &transcript);
        }
    }
}

#[cfg(test)]
#[path = "hook_service_tests.rs"]
mod tests;
