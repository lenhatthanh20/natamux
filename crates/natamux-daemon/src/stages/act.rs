//! `stage-act.ts`: the actions a person or an agent takes on the stages of a task: start, report,
//! send back, approve, skip, pass and fail.

use natamux_core::stages::position::{is_not_taken, step_index};
use natamux_core::stages::terminal_lane::{stage_of_terminal, terminal_of_stage_pinned};
use natamux_proto::stages::capabilities::stage_name_of_id;
use natamux_proto::stages::position::path_of;
use natamux_proto::stages::schema::{Stage, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::{MAX_AGENT_SEND_BACKS, StageAction, lane_state};
use serde_json::{Map, Value, json};

use crate::stages::errors::{StageError, StageResult, action};
use crate::stages::run::{EventInput, StageRun, key};
use crate::tasks::helpers::position_of;

/// What an action names besides its kind.
#[derive(Clone, Debug, Default)]
pub struct StageActionInput {
    /// The stage the action is about. Needed inside a group when more than one could be meant.
    pub stage: Option<String>,
    /// The terminal the action came from (`natamux done` in an agent's terminal).
    pub terminal: Option<String>,
    /// reopen: the stage to go back to.
    pub to: Option<String>,
    /// reopen and fail: why, for the agent and the log.
    pub reason: Option<String>,
    /// The request came with the agent token of a task shell.
    pub by_agent: bool,
}

impl StageRun {
    pub fn act(
        &self,
        task_id: &str,
        what: StageAction,
        input: &StageActionInput,
    ) -> StageResult<Task> {
        let task = self.task(task_id)?;
        let step = self.step_of(&task)?;
        let source = if input.by_agent { "agent" } else { "human" };
        match what {
            StageAction::Start => self.start_action(task_id),
            StageAction::Report => self.report(&task, &step, input, source),
            StageAction::Reopen => self.reopen(&task, &step, input, source),
            other => self.move_stage(&task, &step, other, input, source),
        }
    }

    fn start_action(&self, task_id: &str) -> StageResult<Task> {
        self.options.tasks.set_without_agent(task_id, false)?;
        self.start_lanes(task_id, &[TaskState::Draft, TaskState::Ready], None)?;
        self.task(task_id)
    }

    fn report(
        &self,
        task: &Task,
        step: &Step,
        input: &StageActionInput,
        source: &str,
    ) -> StageResult<Task> {
        let stage = self.resolve_stage(
            task,
            step,
            input,
            &[TaskState::Running, TaskState::NeedsInput],
        )?;
        let reporter = if input.terminal.is_none() {
            source
        } else {
            "agent"
        };
        self.report_and_check(&task.id, &stage.id, reporter, input.terminal.as_deref())?;
        self.task(&task.id)
    }

    fn reopen(
        &self,
        task: &Task,
        step: &Step,
        input: &StageActionInput,
        source: &str,
    ) -> StageResult<Task> {
        let task_id = &task.id;
        if let Some(to) = &input.to {
            self.check_target(task, to)?;
        }
        // A person can type the command in a task shell too; only an agent there counts.
        let by_agent = input
            .terminal
            .as_deref()
            .is_some_and(|terminal| self.agent_runs(task_id, terminal));
        let from = match (by_agent, input.terminal.as_deref()) {
            (true, Some(terminal)) => Some(self.agent_sends_back(task, step, terminal)?),
            _ => None,
        };
        let mut payload = Map::new();
        if let Some(to) = &input.to {
            payload.insert("to".into(), json!(to));
        }
        if let Some(reason) = &input.reason {
            payload.insert("reason".into(), json!(reason));
        }
        if let Some(from) = &from {
            payload.insert("from".into(), json!(from));
            payload.insert("byAgent".into(), json!(true));
        }
        // Its turn ends after this, as after `natamux done`. Noted before the send back, so a
        // shell that it ends waits for that turn end.
        let shell = input
            .terminal
            .as_deref()
            .filter(|_| from.is_some())
            .map(|terminal| key(task_id, terminal));
        let before = match (&shell, &from) {
            (Some(shell), Some(from)) => self.state().reported.insert(shell.clone(), from.clone()),
            _ => None,
        };
        self.state().leaving.insert(task_id.clone(), step.clone());
        let applied = self.apply_action(task_id, "stage.reopened", payload, source);
        self.state().leaving.remove(task_id);
        if let (true, Some(shell)) = (applied.is_err(), &shell) {
            self.restore_report(shell, before);
        }
        applied?;
        self.after_start(task_id)
    }

    fn move_stage(
        &self,
        task: &Task,
        step: &Step,
        what: StageAction,
        input: &StageActionInput,
        source: &str,
    ) -> StageResult<Task> {
        use TaskState::{Draft, NeedsInput, Ready, Review, Running};
        let (kind, states): (&str, &[TaskState]) = match what {
            StageAction::Approve => ("stage.approved", &[Review, NeedsInput]),
            StageAction::Skip => (
                "stage.skipped",
                &[Draft, Ready, Running, NeedsInput, Review],
            ),
            StageAction::Pass => ("stage.passed", &[Ready, Running, Review]),
            _ => ("stage.failed", &[Running]),
        };
        let stage = self.resolve_stage(task, step, input, states)?;
        let mut payload = Map::new();
        payload.insert("stage".into(), json!(stage.id));
        if let Some(reason) = &input.reason {
            payload.insert("reason".into(), json!(reason));
        }
        self.apply_action(&task.id, kind, payload, source)?;
        self.after_start(&task.id)
    }

    /// The stage an action is about: the one named, the one of the terminal, or the only stage of
    /// the step that can take the action. A terminal of a stage that does not run now is refused:
    /// a reviewer that sent the task back must not report Implement done.
    fn resolve_stage(
        &self,
        task: &Task,
        step: &Step,
        input: &StageActionInput,
        states: &[TaskState],
    ) -> StageResult<Stage> {
        let stages = step.stages();
        if let Some(named) = &input.stage {
            return stages
                .iter()
                .find(|stage| &stage.id == named)
                .map(|stage| (*stage).clone())
                .ok_or_else(|| action(format!("{} is not at the {named} stage now", task.id)));
        }
        if let Some(terminal) = &input.terminal {
            let steps = self.steps(task)?;
            let pinned = self.pinned(&task.id);
            if let Some(stage) = stage_of_terminal(&steps, step, terminal, &pinned) {
                return Ok(stage);
            }
            let others: Vec<String> = steps
                .iter()
                .flat_map(|each| each.stages())
                .filter(|stage| terminal_of_stage_pinned(&steps, &stage.id, &pinned) == *terminal)
                .map(|stage| stage.id.clone())
                .collect();
            if !others.is_empty() {
                let what = if others.len() == 1 {
                    format!(
                        "The {} stage of Terminal {terminal} does not run now",
                        others[0]
                    )
                } else {
                    format!("No stage of Terminal {terminal} runs now")
                };
                return Err(action(format!(
                    "{what}: {} is at {}. Stop here.",
                    task.id, task.stage
                )));
            }
        }
        if let Step::Stage(stage) = step {
            return Ok(stage.clone());
        }
        let fitting: Vec<&Stage> = stages
            .iter()
            .copied()
            .filter(|stage| states.contains(&lane_state(task, step, &stage.id)))
            .collect();
        if let [only] = fitting.as_slice() {
            return Ok((*only).clone());
        }
        let ids: Vec<&str> = stages.iter().map(|stage| stage.id.as_str()).collect();
        Err(action(format!(
            "{} runs {} side by side. Name the stage",
            task.id,
            ids.join(" and ")
        )))
    }

    /// A send back goes to this stage or one before it, on the way the task went.
    fn check_target(&self, task: &Task, to: &str) -> StageResult<()> {
        let steps = self.steps(task)?;
        let paths = position_of(task).paths;
        if is_not_taken(&steps, paths.as_ref(), to) {
            return Err(StageError::Input(format!(
                "{to} is on a path this task did not take"
            )));
        }
        let target = step_index(&steps, to);
        let at = step_index(&steps, &task.stage);
        // Stages of one path share the index of their branch.
        let index = |id: &str| path_of(&steps, id).map_or(0, |place| place.index);
        let later = index(to) > index(&task.stage);
        if target < 0 || target > at || (target == at && later) {
            return Err(StageError::Input(format!(
                "{} has no stage \"{to}\" to go back to. Name this stage or one before it",
                task.id
            )));
        }
        Ok(())
    }

    /// An agent that reviews sends the task back by itself, at most MAX_AGENT_SEND_BACKS times
    /// from its stage; then the stage waits for you, so two agents that disagree do not loop.
    fn agent_sends_back(&self, task: &Task, step: &Step, terminal: &str) -> StageResult<String> {
        let steps = self.steps(task)?;
        let pinned = self.pinned(&task.id);
        let from = stage_of_terminal(&steps, step, terminal, &pinned)
            .map_or_else(|| task.stage.clone(), |stage| stage.id);
        let before = self.send_backs_from(&task.id, &from);
        if before >= MAX_AGENT_SEND_BACKS {
            let name = stage_name_of_id(&from);
            let input = EventInput::new(
                "agent",
                "agent.asked",
                json!({
                    "stage": from,
                    "message": format!("{name} sent the task back {before} times. Decide what to do."),
                }),
            );
            self.apply(&task.id, &input)?;
            return Err(action(format!(
                "{} went back {before} times already, so it waits for the user now. Stop here.",
                task.id
            )));
        }
        Ok(from)
    }

    pub fn send_backs_from(&self, task_id: &str, from: &str) -> usize {
        self.events(task_id)
            .iter()
            .filter(|stored| {
                let payload = &stored.event.payload;
                stored.event.kind == "stage.reopened"
                    && payload.get("byAgent") == Some(&Value::Bool(true))
                    && payload.get("from").and_then(Value::as_str) == Some(from)
            })
            .count()
    }

    fn apply_action(
        &self,
        task_id: &str,
        kind: &str,
        payload: Map<String, Value>,
        source: &str,
    ) -> StageResult<()> {
        let input = EventInput {
            source: source.into(),
            kind: kind.into(),
            payload,
        };
        let applied = self.apply_ending(task_id, input)?;
        if !applied.accepted {
            return Err(action(format!(
                "{task_id} cannot take \"{kind}\" now ({})",
                serde_json::to_value(applied.task.state)
                    .ok()
                    .and_then(|state| state.as_str().map(str::to_string))
                    .unwrap_or_default()
            )));
        }
        Ok(())
    }

    /// The task once the stages the action moved it to have started, so the answer shows a stage
    /// that failed at once, not the ready state before it.
    fn after_start(&self, task_id: &str) -> StageResult<Task> {
        // The move's message starts the next step on a thread of its own; it is listed in
        // `start_work` before `apply` returns, as the bus calls the runner first.
        self.wait_for_start(task_id);
        self.task(task_id)
    }
}
