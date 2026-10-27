//! `stage-shells.ts`, live part: which agent runs where, and closing the shell of a stage when it
//! ends. Finishing a task and bringing shells back after a restart are in `shells_finish.rs`.

use std::sync::{Arc, OnceLock};

use natamux_agents::launch::agent_of_command;
use natamux_core::stages::terminal_lane::{ends_shell1, terminal_of_stage_pinned};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::capabilities::can_end_shell;
use natamux_proto::stages::position::step_to_run;
use natamux_proto::stages::schema::{Stage, StageKind, Step};
use natamux_proto::tasks::task::Task;
use serde_json::Value;

use crate::shared::scheduler::TimerId;
use crate::stages::run::{StageRun, key};
use crate::terminals::limits::SHELL_PROGRAMS;

/// After the launch line is typed, the shell stays in the foreground until the agent is up.
pub const LAUNCH_GRACE_MS: i64 = 30_000;

/// The escape sequences that set the window title, as the Windows console passes them on.
fn exited_marks() -> [String; 2] {
    let title = crate::agents::runner::AGENT_EXITED_TITLE;
    [format!("\x1b]0;{title}"), format!("\x1b]2;{title}")]
}

/// Enough of the last output to find a mark that comes in two pieces.
const MARK_TAIL_CHARS: usize = 64;

pub fn is_shell(program: &str) -> bool {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    SHELL_PROGRAMS.contains(&name)
}

impl StageRun {
    /// A stage set to end its shell closes it once its lane is done. This runs before the next
    /// stage starts; a next stage in the same shell closes one that still waits for its agent's
    /// turn end, so it starts a new agent. A finished task is left to `finish`, which keeps the
    /// screens.
    pub fn end_stage_shell(&self, task: &Task, event: &TaskEvent) {
        let Some(stage_id) = event.payload.get("stage").and_then(Value::as_str) else {
            return;
        };
        if task.state.is_finished() {
            return;
        }
        // A planned skip never started the stage, so its shell may hold the agent before it.
        if event.payload.get("planned") == Some(&Value::Bool(true)) {
            return;
        }
        let Ok(steps) = self.steps(task) else {
            return;
        };
        let Some(step) = step_to_run(&steps, stage_id) else {
            return;
        };
        let Some(stage) = step
            .stages()
            .into_iter()
            .find(|each| each.id == stage_id)
            .cloned()
        else {
            return;
        };
        // Still in the step and not done: a stage that waits for approval, or a lane at work.
        let lane_done = task.lanes.get(stage_id).and_then(Value::as_str) == Some("done");
        if task.stage == step.id() && !lane_done {
            return;
        }
        self.close_stage_shell(&task.id, &steps, &stage);
    }

    /// A send back is the end of the stages it leaves, as a pass is. One that goes back inside
    /// the same step leaves none: the stages there run on or run again.
    pub fn end_left_shells(&self, task: &Task) {
        let left = self.state().leaving.get(&task.id).cloned();
        let Some(left) = left.filter(|left| left.id() != task.stage) else {
            return;
        };
        let Ok(steps) = self.steps(task) else {
            return;
        };
        for stage in left.stages() {
            self.close_stage_shell(&task.id, &steps, stage);
        }
    }

    /// Ends the agent or command of a stage set to end its shell, and closes the shell. Shell 1
    /// keeps the task's agent and is never closed. An agent that ended the stage itself, by
    /// `natamux done` or a send back, still runs that command: its shell closes at the turn end.
    fn close_stage_shell(&self, task_id: &str, steps: &[Step], stage: &Stage) {
        let pinned = self.pinned(task_id);
        if stage.end_shell != Some(true)
            || !can_end_shell(stage)
            || ends_shell1(steps, stage, &pinned)
        {
            return;
        }
        let command = stage.kind == StageKind::Testing && stage.command.is_some();
        let terminal = if command {
            self.state().command_terminals.remove(task_id)
        } else {
            Some(terminal_of_stage_pinned(steps, &stage.id, &pinned))
        };
        let Some(terminal) = terminal else {
            return;
        };
        if self.state().reported.contains_key(&key(task_id, &terminal)) {
            self.close_at_turn_end(task_id, &terminal);
        } else {
            self.close_shell_now(task_id, &terminal);
        }
    }

    /// The shell waits for the turn end of its agent (`take_report`), or for the grace time when
    /// that never comes.
    fn close_at_turn_end(&self, task_id: &str, terminal: &str) {
        let scheduler = self.options.terminals.scheduler();
        let run = self.weak();
        let (task, name) = (task_id.to_string(), terminal.to_string());
        let own: Arc<OnceLock<TimerId>> = Arc::default();
        let mine = Arc::clone(&own);
        let timer = scheduler.after(self.options.end_shell_grace_ms, move || {
            if let (Some(run), Some(timer)) = (run.upgrade(), mine.get()) {
                run.close_on_timer(&task, &name, *timer);
            }
        });
        let _ = own.set(timer);
        let before = self.state().closing.insert(key(task_id, terminal), timer);
        if let Some(before) = before {
            scheduler.cancel(before);
        }
    }

    /// The grace time is over. A cancel cannot stop a job the scheduler took already, so a timer
    /// of an earlier round closes nothing: the shell of a newer round waits for its own.
    fn close_on_timer(&self, task_id: &str, terminal: &str, timer: TimerId) {
        let shell = key(task_id, terminal);
        let mine = {
            let mut state = self.state();
            let mine = state.closing.get(&shell) == Some(&timer);
            if mine {
                state.closing.remove(&shell);
            }
            mine
        };
        if mine {
            self.close_shell_now(task_id, terminal);
        }
    }

    /// Closes now a shell that waits for the turn end of its agent: the turn ended, or a next
    /// stage starts in it.
    pub fn close_pending_shell(&self, task_id: &str, terminal: &str) {
        let timer = self.state().closing.remove(&key(task_id, terminal));
        if let Some(timer) = timer {
            self.options.terminals.scheduler().cancel(timer);
            self.close_shell_now(task_id, terminal);
        }
    }

    fn close_shell_now(&self, task_id: &str, terminal: &str) {
        self.options.terminals.close_one(task_id, terminal);
        self.forget_shell(task_id, terminal);
    }

    /// What the runner knows of a shell the daemon ended: the next agent there is a new one.
    pub fn forget_shell(&self, task_id: &str, terminal: &str) {
        // The killed agent never ends its turn, so its report would swallow the first turn end
        // of the next agent in this shell.
        let shell = key(task_id, terminal);
        let mut state = self.state();
        state.reported.remove(&shell);
        state.launched.remove(&shell);
        state.output_tails.remove(&shell);
    }

    /// A prompt goes into a running agent only. Pasted into a shell, its lines would run as
    /// commands. An agent runs when the program in the foreground is not a shell (an agent
    /// started here, by hand, or before a restart), or for a short time after this daemon
    /// started one.
    pub fn agent_runs(&self, task_id: &str, terminal: &str) -> bool {
        let shell = key(task_id, terminal);
        if !self.options.terminals.is_open(task_id, terminal) {
            // A new shell with this name has no agent in it.
            let mut state = self.state();
            state.launched.remove(&shell);
            state.output_tails.remove(&shell);
            return false;
        }
        let launched_at = self
            .state()
            .launched
            .get(&shell)
            .map(|launched| launched.at);
        let program = self.options.terminals.foreground(task_id, terminal);
        if program.as_deref().is_some_and(|program| !is_shell(program)) {
            return true;
        }
        let Some(at) = launched_at else {
            return false;
        };
        // Some platforms name only the shell (Windows does), so an agent started here counts.
        program.is_none() || self.options.windows || self.now() - at < LAUNCH_GRACE_MS
    }

    /// The agent that runs in a terminal, for its pane header: the one its processes show.
    /// Where the platform does not list them, or just after the launch, the one this daemon
    /// started there. None for a program that is no agent Natamux knows. May run `ps`.
    pub fn agent_in(&self, task_id: &str, terminal: &str) -> Option<String> {
        let commands = self.options.terminals.commands(task_id, terminal);
        if let Some(agent) = self.agent_of_commands(commands.as_deref()) {
            return Some(agent);
        }
        let launched = self
            .state()
            .launched
            .get(&key(task_id, terminal))
            .cloned()?;
        (commands.is_none() || self.now() - launched.at < LAUNCH_GRACE_MS).then_some(launched.agent)
    }

    pub fn agent_of_commands(&self, commands: Option<&[String]>) -> Option<String> {
        let program_of = |agent: &str| self.options.agents.program_of(agent);
        commands?
            .iter()
            .find_map(|line| agent_of_command(line, program_of))
            .map(str::to_string)
    }

    /// Output of a terminal of a task. On Windows an agent started here counts as running until
    /// its launch line sets the window title that says it has ended (/exit, a crash). Nothing
    /// else runs on the output path.
    pub fn on_output(&self, task_id: &str, terminal: &str, data: &[u8]) {
        if !self.options.windows {
            return;
        }
        let shell = key(task_id, terminal);
        let mut state = self.state();
        if !state.launched.contains_key(&shell) {
            return;
        }
        let tail = state.output_tails.get(&shell).cloned().unwrap_or_default();
        let text = tail + &String::from_utf8_lossy(data);
        if exited_marks()
            .iter()
            .any(|mark| text.contains(mark.as_str()))
        {
            state.launched.remove(&shell);
            state.output_tails.remove(&shell);
            return;
        }
        let start = text
            .char_indices()
            .rev()
            .nth(MARK_TAIL_CHARS - 1)
            .map_or(0, |(index, _)| index);
        state.output_tails.insert(shell, text[start..].to_string());
    }
}
