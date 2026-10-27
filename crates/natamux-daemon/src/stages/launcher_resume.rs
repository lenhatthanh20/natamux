//! `stage-launcher.ts`, the starts that are not a stage's first: a resume, the agents that come
//! back when the daemon starts, and an agent started by a person.

use natamux_agents::launch::{LAST_SESSION, is_launchable};
use natamux_core::stages::terminal_lane::{stage_agent, terminal_of_stage_pinned};
use natamux_core::tasks::dependencies::waiting_on;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::position::find_step;
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::{KEEPS_TURN, RESUMED_STATES, lane_state, run_by_agent};
use natamux_stages::ci::stages_waiting_for_ci;
use serde_json::{Value, json};

use crate::agents::runner::OwnAgent;
use crate::stages::errors::StageResult;
use crate::stages::launcher::GO_ON_PROMPT;
use crate::stages::run::{EventInput, StageRun};
use crate::terminals::limits::MAIN_TERMINAL;

impl StageRun {
    /// A person resumed a stage that waited for them. Its agent may be gone, for example after a
    /// restart, and then starts again. One that still runs and ended its turn waits at its
    /// prompt, so it is asked to go on. One that asked gets nothing: the Enter would answer its
    /// menu.
    pub fn resume_stage(&self, task_id: &str, event: &TaskEvent) {
        if let Err(error) = self.resume_stage_once(task_id, event) {
            eprintln!("Could not resume the stage of {task_id}: {error}");
        }
    }

    fn resume_stage_once(&self, task_id: &str, event: &TaskEvent) -> StageResult<()> {
        let task = self.task(task_id)?;
        let step = self.step_of(&task)?;
        let wanted = event.payload.get("stage").and_then(Value::as_str);
        let Some(stage) = step
            .stages()
            .into_iter()
            .find(|candidate| Some(candidate.id.as_str()) == wanted)
            .cloned()
        else {
            return Ok(());
        };
        let terminal = self.terminal_of(&task, &stage.id)?;
        if !self.agent_runs(&task.id, &terminal) {
            self.relaunch(&task, &step, &stage, false, false)?;
        } else if self.ended_turn(&task.id, &stage.id) {
            self.options
                .terminals
                .paste(&task.id, GO_ON_PROMPT, &terminal)?;
        }
        Ok(())
    }

    /// True when the last thing the stage's agent did was to end its turn. Only the agent's own
    /// events count: a CI result, an edit, a restart's launch or this resume leave it there.
    fn ended_turn(&self, task_id: &str, stage_id: &str) -> bool {
        for stored in self.events(task_id).iter().rev() {
            let event = &stored.event;
            let stage = event.payload.get("stage").and_then(Value::as_str);
            let other_stage = stage.is_some_and(|stage| stage != stage_id);
            if !stored.accepted
                || other_stage
                || event.source != "agent"
                || KEEPS_TURN.contains(&event.kind.as_str())
            {
                continue;
            }
            // After an API error the agent waits at its prompt, as after a turn end.
            return event.kind == "agent.finished" || event.kind == "agent.failed";
        }
        false
    }

    /// Agents end with the daemon. At its start, each stage an agent worked on gets its agent
    /// back, and the task keeps its state. A stage in review gets it too, so you can ask it for
    /// changes.
    pub fn resume_agents(&self) {
        let all = self.all_tasks();
        for task in &all {
            if task.state.is_finished() {
                continue;
            }
            // A stage the settings no longer have: the task waits for a person, the daemon starts.
            let Ok(step) = self.step_of(task) else {
                continue;
            };
            for stage in step.stages() {
                if let Err(error) = self.resume_lane(task, &step, stage, &all) {
                    eprintln!("Could not bring back the agent of {}: {error}", task.id);
                }
            }
        }
        if let Some(open) = &self.options.open_shells {
            open.release_held();
        }
    }

    fn resume_lane(
        &self,
        task: &Task,
        step: &Step,
        stage: &Stage,
        all: &[Task],
    ) -> StageResult<()> {
        let state = lane_state(task, step, &stage.id);
        // A stage that waited for its workspace or for other tasks waits again: the restart
        // emptied those lists. Checked in the order start_agent and start_command check.
        if state == TaskState::Ready && matches!(stage.kind, StageKind::Agent | StageKind::Testing)
        {
            self.wait_again(task, stage, all);
            return Ok(());
        }
        // The restart killed the shell of a test command, so its result never comes. It runs
        // again. A stage that waits for CI is resume_ci_checks' work.
        if stage.kind == StageKind::Testing
            && stage.command.is_some()
            && state == TaskState::Running
            && !stages_waiting_for_ci(&self.events(&task.id)).contains(&stage.id)
        {
            return self.run_command_again(task, step, stage);
        }
        let working = RESUMED_STATES.contains(&state);
        // No agent Natamux started for it, or a task left to a person: a person runs their own
        // in the shell.
        let agent = if task.without_agent == Some(true) {
            None
        } else {
            stage_agent(step, stage)
                .agent
                .or_else(|| task.agent.clone())
        };
        // An agent the person quit before the app closed stays quit; the stage can be resumed.
        // A paused agent waits until someone opens its task.
        let terminal = self.terminal_of(task, &stage.id)?;
        let stays = self.options.open_shells.as_ref().is_some_and(|open| {
            !open.was_running(&task.id, &terminal) || open.is_paused(&task.id, &terminal)
        });
        if working && agent.as_deref().is_some_and(is_launchable) && !stays {
            // In a group, the newest session of the folder may be the other stage's.
            self.relaunch(task, step, stage, !step.is_group(), true)?;
        }
        Ok(())
    }

    fn wait_again(&self, task: &Task, stage: &Stage, all: &[Task]) {
        if task.workspace.is_none() {
            self.state().waiting_for_workspace.insert(task.id.clone());
        } else if run_by_agent(stage) && !waiting_on(task, all).is_empty() {
            self.state().waiting_for_tasks.insert(task.id.clone());
        }
    }

    fn run_command_again(&self, task: &Task, step: &Step, stage: &Stage) -> StageResult<()> {
        let stopped = self.apply_system(&task.id, "agent.stopped", json!({ "stage": stage.id }))?;
        if stopped.accepted {
            let (task_id, step, stage) = (task.id.clone(), step.clone(), stage.clone());
            self.background(move |run| {
                if let Err(error) = run.start_stage(&task_id, &step, &stage, None) {
                    eprintln!("Could not run the command of {task_id} again: {error}");
                }
            });
        }
        Ok(())
    }

    /// Starts the agent of a stage again when it is gone. It goes on with the last session it
    /// reported in its shell; Claude Code also with its session of the stage, or with `newest`
    /// the newest of the folder. With none, it gets the stage prompt again. At the daemon's start
    /// (`restart`) its pane shows a spinner until the agent runs.
    fn relaunch(
        &self,
        task: &Task,
        step: &Step,
        stage: &Stage,
        newest: bool,
        restart: bool,
    ) -> StageResult<()> {
        if !run_by_agent(stage) || task.workspace.is_none() {
            return Ok(());
        }
        let terminal = self.terminal_of(task, &stage.id)?;
        if self.agent_runs(&task.id, &terminal) {
            return Ok(());
        }
        let own = stage_agent(step, stage);
        let agent = own
            .agent
            .clone()
            .or_else(|| task.agent.clone())
            .unwrap_or_else(|| "claude-code".into());
        let session = self
            .options
            .open_shells
            .as_ref()
            .and_then(|open| open.session_of(&task.id, &terminal, &agent))
            .or_else(|| self.last_session(task, &stage.id, &agent, &terminal))
            .or_else(|| (newest && agent == "claude-code").then(|| LAST_SESSION.to_string()));
        let text = match session {
            None => (self.prompt_of(task, stage, None))(&agent).text,
            Some(_) => String::new(),
        };
        self.launch(
            &task.id,
            &stage.id,
            &agent,
            &terminal,
            &text,
            session.as_deref(),
            &own,
        )?;
        if restart && let Some(open) = &self.options.open_shells {
            open.mark_starting(&task.id, &terminal, &agent);
        }
        Ok(())
    }

    /// The last session that Claude Code reported from this stage. Other agents start new ones.
    pub fn last_session(
        &self,
        task: &Task,
        stage_id: &str,
        agent: &str,
        terminal: &str,
    ) -> Option<String> {
        if agent != "claude-code" {
            return None;
        }
        let steps = self.steps(task).ok()?;
        let pinned = self.pinned(&task.id);
        let shell_of =
            |id: &str| find_step(&steps, id).map(|_| terminal_of_stage_pinned(&steps, id, &pinned));
        for stored in self.events(&task.id).iter().rev() {
            let event = &stored.event;
            // A session from before another agent was started in this shell is not this agent's.
            // The own agent of a stage in another shell does not count.
            let launch_stage = event.payload.get("stage").and_then(Value::as_str);
            let launch_shell = event.payload.get("terminal").and_then(Value::as_str);
            let other_shell = launch_stage.is_some_and(|stage| {
                stage != stage_id && shell_of(stage).as_deref() != Some(terminal)
            }) || launch_shell.is_some_and(|shell| shell != terminal);
            let other_agent = event.payload.get("agent").and_then(Value::as_str) != Some(agent);
            if event.kind == "agent.launched" && other_agent && !other_shell {
                return None;
            }
            let session = event.payload.get("sessionId").and_then(Value::as_str);
            let from_stage = event.source == "agent" && launch_stage == Some(stage_id);
            if let (true, Some(session)) = (from_stage, session)
                && !session.is_empty()
            {
                return Some(session.into());
            }
        }
        None
    }

    /// An agent started by a person: Start agent in the dashboard or the CLI. A stage that waits
    /// for its agent starts it, with the stage prompt. An agent that runs already is left alone,
    /// since its command would be typed into it; a prompt goes to it instead. A task with no
    /// workspace gets one from `make_workspace` first.
    pub fn start_by_hand(
        &self,
        task_id: &str,
        agent: &str,
        prompt: Option<&str>,
        make_workspace: Option<&dyn Fn() -> StageResult<()>>,
    ) -> StageResult<Task> {
        self.options.tasks.set_without_agent(task_id, false)?;
        if let Some(make) = make_workspace
            && self.task(task_id)?.workspace.is_none()
        {
            // The new workspace starts the stage that waits for it. It must start the agent
            // picked here, not the task's own.
            self.start_in_new_workspace(task_id, agent, make)?;
        }
        // A new workspace may be starting the stage right now.
        self.wait_for_start(task_id);
        if prompt.is_none() && self.continue_failed(task_id)? {
            return self.task(task_id);
        }
        if prompt.is_none() && !self.agent_runs(task_id, MAIN_TERMINAL) {
            self.start_lanes_with(task_id, agent)?;
        }
        self.prompt_or_start(task_id, agent, prompt)
    }

    fn start_in_new_workspace(
        &self,
        task_id: &str,
        agent: &str,
        make_workspace: &dyn Fn() -> StageResult<()>,
    ) -> StageResult<()> {
        self.state()
            .chosen_agents
            .insert(task_id.into(), agent.into());
        let made = make_workspace();
        if made.is_ok() {
            self.wait_for_start(task_id);
        }
        self.state().chosen_agents.remove(task_id);
        made
    }

    /// The stages that wait for an agent start the one a person picked.
    fn start_lanes_with(&self, task_id: &str, agent: &str) -> StageResult<()> {
        self.state()
            .chosen_agents
            .insert(task_id.into(), agent.into());
        let started = self.start_lanes(task_id, &[TaskState::Draft, TaskState::Ready], None);
        self.state().chosen_agents.remove(task_id);
        started
    }

    /// The prompt goes to the agent that runs. With none, the picked agent starts in Shell 1.
    fn prompt_or_start(
        &self,
        task_id: &str,
        agent: &str,
        prompt: Option<&str>,
    ) -> StageResult<Task> {
        if self.agent_runs(task_id, MAIN_TERMINAL) {
            if let Some(prompt) = prompt {
                self.options
                    .terminals
                    .paste(task_id, prompt, MAIN_TERMINAL)?;
            }
            return self.task(task_id);
        }
        let task =
            self.options
                .agents
                .start(task_id, agent, prompt, None, None, &OwnAgent::default())?;
        self.note_launch(task_id, MAIN_TERMINAL, agent);
        Ok(task)
    }

    /// The stages of the current step whose agent failed go on where they stopped, as Resume
    /// does. False when no such stage failed.
    fn continue_failed(&self, task_id: &str) -> StageResult<bool> {
        let task = self.task(task_id)?;
        let step = self.step_of(&task)?;
        let failed: Vec<String> = step
            .stages()
            .into_iter()
            .filter(|stage| {
                run_by_agent(stage) && lane_state(&task, &step, &stage.id) == TaskState::Failed
            })
            .map(|stage| stage.id.clone())
            .collect();
        for stage in &failed {
            let input = EventInput::new("human", "agent.resumed", json!({ "stage": stage }));
            self.apply(task_id, &input)?;
        }
        Ok(!failed.is_empty())
    }
}
