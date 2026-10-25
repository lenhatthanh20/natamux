//! `stage-shells.ts`, the ends and the starts: a finished task keeps the last screens of its
//! shells, and after a restart the shells that were open come back.

use std::collections::{HashMap, HashSet};

use natamux_agents::launch::LAST_SESSION;
use natamux_core::js::iso_from_millis;
use natamux_core::stages::terminal_lane::terminal_of_stage_pinned;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::ended::EndedShell;
use natamux_stages::actions::{RESUMED_STATES, lane_state};
use serde_json::Value;

use crate::agents::runner::OwnAgent;
use crate::stages::errors::{StageResult, action};
use crate::stages::run::{StageRun, key};
use crate::stages::shells::is_shell;
use crate::terminals::limits::{ENDED_SHELL_LINES, MAIN_TERMINAL};
use crate::terminals::shell_files::{read_ended_shells, save_ended_shells};

/// The agent this daemon started in a shell, and the session it last reported there.
#[derive(Clone, Debug)]
struct Started {
    agent: String,
    session: Option<String>,
}

impl StageRun {
    /// A task that is done or cancelled needs its agent and its shells no more. A deleted task
    /// keeps nothing; a finished one keeps the last screens of its shells for the wrap-up. Called
    /// from the bus: the screens are kept on a thread of its own.
    pub fn finish(&self, task_id: &str, keep_screens: bool) {
        let mut started = HashMap::new();
        for terminal in self.options.terminals.list(task_id) {
            let launched = self.state().launched.get(&key(task_id, &terminal)).cloned();
            if let Some(launched) = launched {
                // Read now: the list of open shells forgets the task below.
                let session = self
                    .options
                    .open_shells
                    .as_ref()
                    .and_then(|open| open.session_of(task_id, &terminal, &launched.agent));
                let agent = launched.agent;
                started.insert(terminal, Started { agent, session });
            }
        }
        if keep_screens {
            let task_id = task_id.to_string();
            self.background(move |run| run.end_shells(&task_id, &started));
        } else {
            self.options.terminals.close(task_id);
        }
        let prefix = key(task_id, "");
        let mut state = self.state();
        state
            .launched
            .retain(|shell, _| !shell.starts_with(&prefix));
        state
            .reported
            .retain(|shell, _| !shell.starts_with(&prefix));
        state
            .output_tails
            .retain(|shell, _| !shell.starts_with(&prefix));
        state.command_terminals.remove(task_id);
        state.waiting_for_workspace.remove(task_id);
        state.waiting_for_tasks.remove(task_id);
        let mut timers = Vec::new();
        state.closing.retain(|shell, timer| {
            let other = !shell.starts_with(&prefix);
            if !other {
                timers.push(*timer);
            }
            other
        });
        drop(state);
        // The shells end with the task: no later timer may close a shell of a new round.
        let scheduler = self.options.terminals.scheduler();
        for timer in timers {
            scheduler.cancel(timer);
        }
        if let Some(open) = &self.options.open_shells {
            open.forget(task_id);
        }
    }

    /// Keeps the last screen of each shell, then ends it. A shell where a program that is no
    /// agent runs, such as a dev server, stays until it is idle. Without a list of processes
    /// nothing tells an agent from a dev server, so such a shell ends, as all of them did before.
    fn end_shells(&self, task_id: &str, started: &HashMap<String, Started>) {
        let ended_at = iso_from_millis(self.now());
        let finishing = self.all_tasks().into_iter().find(|each| each.id == task_id);
        let stages = finishing
            .as_ref()
            .map(|task| self.last_stages(task))
            .unwrap_or_default();
        let mut shells = Vec::new();
        let mut kept = HashSet::new();
        for terminal in self.options.terminals.list(task_id) {
            // The screen parses at once: only the last batch of output waits.
            self.options.terminals.settled(task_id, &terminal);
            let commands = self.options.terminals.commands(task_id, &terminal);
            let Some(text) =
                self.options
                    .terminals
                    .last_screen(task_id, &terminal, ENDED_SHELL_LINES)
            else {
                // Closed meanwhile.
                continue;
            };
            let own = started.get(&terminal);
            let agent = own
                .map(|own| own.agent.clone())
                .or_else(|| self.agent_of_commands(commands.as_deref()));
            let program = self.options.terminals.foreground(task_id, &terminal);
            let busy = program.as_deref().is_some_and(|program| !is_shell(program));
            if busy && agent.is_none() && commands.is_some() {
                kept.insert(terminal.clone());
            }
            shells.push(EndedShell {
                name: terminal.clone(),
                agent,
                stage: stages.get(&terminal).cloned(),
                session: own.and_then(|own| own.session.clone()),
                ended_at: ended_at.clone(),
                text,
            });
        }
        // A task deleted meanwhile keeps nothing, and its shells are closed already. One started
        // again meanwhile may use these shells.
        let task = self.all_tasks().into_iter().find(|each| each.id == task_id);
        if !task.is_some_and(|task| task.state.is_finished()) {
            return;
        }
        // Before the shells end: their end tells the dashboards to read the screens. The first
        // finish wins, so a later event does not replace the screens with newer, empty shells.
        if !shells.is_empty()
            && read_ended_shells(self.data_dir(), task_id).is_empty()
            && let Err(error) = save_ended_shells(self.data_dir(), task_id, &shells)
        {
            eprintln!("Could not keep the last screens of {task_id}: {error}");
        }
        for shell in &shells {
            if kept.contains(&shell.name) {
                self.options.terminals.end_when_idle(task_id, &shell.name);
            } else {
                self.options.terminals.close_one(task_id, &shell.name);
            }
        }
    }

    /// The stage that started last in each shell, from the task's events. A start before
    /// terminals were pinned names none: the settings give it.
    pub fn last_stages(&self, task: &Task) -> HashMap<String, String> {
        let steps = self.steps(task).unwrap_or_default();
        let mut stages = HashMap::new();
        for stored in self.events(&task.id) {
            let payload = &stored.event.payload;
            let stage = payload.get("stage").and_then(Value::as_str);
            if let (true, "stage.started", Some(stage)) =
                (stored.accepted, stored.event.kind.as_str(), stage)
            {
                let terminal = payload.get("terminal").and_then(Value::as_str).map_or_else(
                    || terminal_of_stage_pinned(&steps, stage, &Vec::new()),
                    str::to_string,
                );
                stages.insert(terminal, stage.to_string());
            }
        }
        stages
    }

    /// At the daemon's start, before the dashboards connect: the shells that were open come back
    /// once a dashboard opens their task. Those of the stages whose agent `resume_agents` starts
    /// at once are held until it ran. A finished task, or one without a workspace, keeps none.
    pub fn load_open_shells(&self) {
        let Some(open) = &self.options.open_shells else {
            return;
        };
        for task in self.all_tasks() {
            if task.state.is_finished() || task.workspace.is_none() {
                open.forget(&task.id);
                continue;
            }
            // A stage the settings no longer have: every shell waits for a dashboard.
            let working = self.working_terminals(&task).unwrap_or_default();
            open.load(&task.id, &working);
        }
    }

    fn working_terminals(&self, task: &Task) -> StageResult<Vec<String>> {
        let steps = self.steps(task)?;
        let step = self.step_of(task)?;
        let pinned = self.pinned(&task.id);
        Ok(step
            .stages()
            .into_iter()
            .filter(|stage| RESUMED_STATES.contains(&lane_state(task, &step, &stage.id)))
            .map(|stage| terminal_of_stage_pinned(&steps, &stage.id, &pinned))
            .collect())
    }

    /// A dashboard attaches to a shell of a task, so the task is open: every agent of it that
    /// waits to come back after a restart goes on with its session, with no prompt, the attached
    /// shell's first. A plain shell is left to its own attach.
    pub fn restore_task(&self, task_id: &str, attached: &str) {
        let Some(open) = &self.options.open_shells else {
            return;
        };
        let waiting = open.waiting(task_id);
        let others = waiting.iter().filter(|name| name.as_str() != attached);
        for terminal in std::iter::once(&attached.to_string()).chain(others) {
            self.restore_shell(task_id, terminal, terminal == attached);
        }
    }

    /// An agent that cannot start: the attached shell starts plain and keeps its agent in the
    /// list for the next start; any other shell waits again.
    fn restore_shell(&self, task_id: &str, terminal: &str, attached: bool) {
        let Some(open) = &self.options.open_shells else {
            return;
        };
        let restoring = open.restoring(task_id);
        let has_agent = restoring
            .iter()
            .any(|(name, agent)| name == terminal && agent.is_some());
        if !has_agent {
            return;
        }
        let Some(shell) = open.take(task_id, terminal) else {
            return;
        };
        let Some(agent) = shell.agent else {
            return;
        };
        let session = shell.session.unwrap_or_else(|| LAST_SESSION.into());
        let own = OwnAgent {
            restored: true,
            ..OwnAgent::default()
        };
        let started =
            self.options
                .agents
                .start(task_id, &agent, None, Some(terminal), Some(&session), &own);
        match started {
            Ok(_) => {
                self.note_launch(task_id, terminal, &agent);
                open.mark_starting(task_id, terminal, &agent);
            }
            Err(error) => {
                eprintln!("Terminal {terminal} of {task_id} comes back without its agent: {error}");
                if !attached {
                    open.put_back(task_id, terminal);
                }
            }
        }
    }

    /// A person reopens the agent of Terminal 1 of a done or cancelled task, to ask it
    /// something. It goes on with its last session and gets no prompt. The task keeps its state
    /// and its agent.
    pub fn reopen_agent(&self, task_id: &str) -> StageResult<Task> {
        let task = self.task(task_id)?;
        if !task.state.is_finished() {
            return Err(action(format!("{task_id} is not done or cancelled")));
        }
        if task.workspace.is_none() {
            return Err(action(format!("{task_id} has no workspace")));
        }
        if self.agent_runs(task_id, MAIN_TERMINAL) {
            return Err(action(format!(
                "An agent runs in Terminal 1 of {task_id} already"
            )));
        }
        let ended = read_ended_shells(self.data_dir(), task_id)
            .into_iter()
            .find(|shell| shell.name == MAIN_TERMINAL && shell.agent.is_some());
        let agent = ended
            .as_ref()
            .and_then(|shell| shell.agent.clone())
            .or_else(|| task.agent.clone())
            .ok_or_else(|| action(format!("No agent ran in Terminal 1 of {task_id}")))?;
        // The session it reported, else Claude Code's last one of the stage that ran there last,
        // else the newest of the folder.
        let stage = ended
            .as_ref()
            .and_then(|shell| shell.stage.clone())
            .or_else(|| self.last_stages(&task).remove(MAIN_TERMINAL));
        let of_stage =
            stage.and_then(|stage| self.last_session(&task, &stage, &agent, MAIN_TERMINAL));
        let session = ended
            .and_then(|shell| shell.session)
            .or(of_stage)
            .unwrap_or_else(|| LAST_SESSION.into());
        let own = OwnAgent {
            restored: true,
            ..OwnAgent::default()
        };
        let started = self.options.agents.start(
            task_id,
            &agent,
            None,
            Some(MAIN_TERMINAL),
            Some(&session),
            &own,
        )?;
        self.note_launch(task_id, MAIN_TERMINAL, &agent);
        if let Some(open) = &self.options.open_shells {
            open.mark_starting(task_id, MAIN_TERMINAL, &agent);
        }
        Ok(started)
    }
}
