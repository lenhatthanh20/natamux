//! `stage-launcher.ts`, start part: starts the agents and commands of stages. Starts after a
//! restart, a resume or by hand are in `launcher_resume.rs`.

use std::io::Write;
use std::path::{Path, PathBuf};

use natamux_agents::launch::{Platform, quote_argument};
use natamux_agents::skill_resolve::{SkillContext, fill_skills};
use natamux_core::stages::capabilities::runs_workflow;
use natamux_core::stages::terminal_lane::{
    StageAgent, stage_agent, terminal_at_start, terminal_of_stage_pinned,
};
use natamux_core::tasks::dependencies::{open_subtasks, waiting_on};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{Stage, StageKind};
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_stages::actions::send_back_reason;
use natamux_stages::checks::current_head;
use natamux_stages::files::exclude_stage_dir;
use natamux_stages::git::pull_fast_forward;
use natamux_stages::prompts::stage_prompt;
use serde_json::{Map, Value, json};

use crate::agents::runner::OwnAgent;
use crate::stages::errors::StageResult;
use crate::stages::run::{Launched, StageRun, key};
use crate::terminals::types::DEFAULT_SIZE;

/// What Resume types to an agent that still runs and ended its turn.
pub const GO_ON_PROMPT: &str = "Go on with the stage you are working on.";

/// A stage prompt for the agent that will get it, the skills it names that were not found, and
/// whether it asks the agent for a workflow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prompt {
    pub text: String,
    pub missing: Vec<String>,
    pub workflow: bool,
}

pub type PromptFor = Box<dyn Fn(&str) -> Prompt + Send + Sync>;

impl StageRun {
    /// The stage's terminal is pinned in the event: lookups use it until the stage starts again.
    pub fn mark_started(
        &self,
        task: &Task,
        stage: &Stage,
        extra: Map<String, Value>,
    ) -> StageResult<bool> {
        let terminal = self.start_terminal(task, &stage.id)?;
        let mut payload = Map::new();
        payload.insert("stage".into(), json!(stage.id));
        payload.insert("terminal".into(), json!(terminal));
        payload.extend(extra);
        let started = self
            .apply_system(&task.id, "stage.started", Value::Object(payload))?
            .accepted;
        // The turn end of an earlier report of this stage never came; it must not swallow one
        // of this round.
        let mut state = self.state();
        let terminal_key = key(&task.id, &terminal);
        if started && state.reported.get(&terminal_key) == Some(&stage.id) {
            state.reported.remove(&terminal_key);
        }
        Ok(started)
    }

    pub fn start_terminal(&self, task: &Task, stage_id: &str) -> StageResult<String> {
        let steps = self.steps(task)?;
        Ok(terminal_at_start(&steps, stage_id, &self.pinned(&task.id)))
    }

    pub fn terminal_of(&self, task: &Task, stage_id: &str) -> StageResult<String> {
        let steps = self.steps(task)?;
        Ok(terminal_of_stage_pinned(
            &steps,
            stage_id,
            &self.pinned(&task.id),
        ))
    }

    pub fn skill_context(&self, task: &Task, agent: &str) -> SkillContext {
        let repo = self.repo_path(task);
        SkillContext {
            agent: agent.into(),
            workspace: task
                .workspace
                .as_ref()
                .map_or_else(|| repo.clone(), PathBuf::from),
            repo,
            home: self.options.home.clone(),
        }
    }

    /// The prompt of a stage that an agent works on: an agent stage, or a testing stage with no
    /// command.
    pub fn prompt_of(&self, task: &Task, stage: &Stage, cause: Option<&TaskEvent>) -> PromptFor {
        let run = self.weak();
        let (task, stage) = (task.clone(), stage.clone());
        if stage.kind != StageKind::Agent {
            return Box::new(move |agent| {
                let Some(run) = run.upgrade() else {
                    return Prompt::default();
                };
                let context = run.skill_context(&task, agent);
                let filled = fill_skills(&stage_prompt(&stage, &task, false), &context);
                Prompt {
                    text: filled.text,
                    missing: filled.missing,
                    workflow: false,
                }
            });
        }
        let reason = send_back_reason(cause);
        Box::new(move |agent| {
            let Some(run) = run.upgrade() else {
                return Prompt::default();
            };
            let workflow = runs_workflow(&stage, &task.workflow_stages, Some(agent));
            let context = run.skill_context(&task, agent);
            let filled = fill_skills(&stage_prompt(&stage, &task, workflow), &context);
            let text = match &reason {
                Some(reason) => format!("{}\nWhat to change: {reason}", filled.text),
                None => filled.text,
            };
            Prompt {
                text,
                missing: filled.missing,
                workflow,
            }
        })
    }

    pub fn start_agent(
        &self,
        task: &Task,
        stage: &Stage,
        prompt_for: PromptFor,
        resumed: bool,
    ) -> StageResult<()> {
        let Some(workspace) = task.workspace.clone() else {
            self.state().waiting_for_workspace.insert(task.id.clone());
            return Ok(());
        };
        // The agent may not start before the tasks it depends on are done, nor a new stage
        // before the open subtasks, whose work it goes on with. The stage stays ready and starts
        // when they are.
        let all = self.all_tasks();
        if !waiting_on(task, &all).is_empty() || (!resumed && !open_subtasks(task, &all).is_empty())
        {
            self.state().waiting_for_tasks.insert(task.id.clone());
            return Ok(());
        }
        let chosen = self.state().chosen_agents.get(&task.id).cloned();
        if task.without_agent == Some(true) && chosen.is_none() {
            return Ok(());
        }
        let workspace = Path::new(&workspace);
        let _ = exclude_stage_dir(workspace);
        // Before the head is read, so the stage's own commits do not count the pulled ones.
        let pull_error = if resumed {
            None
        } else {
            self.pull_subtasks(task, workspace, &all)
        };
        let head = current_head(workspace);
        let own = stage_agent(stage);
        let agent = own
            .agent
            .clone()
            .or(chosen)
            .or_else(|| task.agent.clone())
            .unwrap_or_else(|| "claude-code".into());
        let terminal = self.start_terminal(task, &stage.id)?;
        // A prompt typed to the agent took the stage up again: the agent has its prompt. With no
        // agent left (the app restarted), Claude Code goes on with its last session of the stage.
        let running = resumed && self.agent_runs(&task.id, &terminal);
        let session = if resumed && !running {
            self.last_session(task, &stage.id, &agent, &terminal)
        } else {
            None
        };
        let prompt = (!running && session.is_none()).then(|| prompt_for(&agent));
        let mut extra = Map::new();
        extra.insert("head".into(), json!(head));
        if let Some(prompt) = prompt.as_ref().filter(|prompt| !prompt.missing.is_empty()) {
            extra.insert("missingSkills".into(), json!(prompt.missing));
            extra.insert("agent".into(), json!(agent));
        }
        if prompt.as_ref().is_some_and(|prompt| prompt.workflow) {
            extra.insert("workflow".into(), json!(true));
        }
        if !self.mark_started(task, stage, extra)? || running {
            return Ok(());
        }
        if let Some(error) = pull_error {
            let reason = format!("Could not pull the work of the subtasks: {error}");
            return self.fail_stage(&task.id, &stage.id, &reason);
        }
        let text = prompt.map(|prompt| prompt.text).unwrap_or_default();
        self.launch(
            &task.id,
            &stage.id,
            &agent,
            &terminal,
            &text,
            session.as_deref(),
            &own,
        )
    }

    /// Merged subtasks are in the remote copy of the task's branch: the stage goes on with them.
    /// Gives git's message when the pull fails.
    fn pull_subtasks(&self, task: &Task, workspace: &Path, all: &[Task]) -> Option<String> {
        let merged = all.iter().any(|other| {
            other.parent_id.as_deref() == Some(task.id.as_str()) && other.state == TaskState::Done
        });
        if !merged {
            return None;
        }
        pull_fast_forward(workspace)
            .err()
            .map(|error| error.to_string())
    }

    /// Gives the prompt to the agent that runs in the terminal, else starts the agent there.
    #[allow(clippy::too_many_arguments)]
    pub fn launch(
        &self,
        task_id: &str,
        stage_id: &str,
        agent: &str,
        terminal: &str,
        text: &str,
        session: Option<&str>,
        own: &StageAgent,
    ) -> StageResult<()> {
        // The agent of the stage before still ends its turn there; this stage needs a new one.
        self.close_pending_shell(task_id, terminal);
        let paused = self.paused_session(task_id, terminal, agent);
        let session = session.or(paused.as_deref());
        let result = if session.is_none() && self.agent_runs(task_id, terminal) {
            self.options
                .terminals
                .paste(task_id, text, terminal)
                .map_err(|error| error.to_string())
        } else {
            let none = own.agent.is_none() && own.args.is_none();
            let own_agent = if none {
                OwnAgent::default()
            } else {
                OwnAgent {
                    args: own.args.clone(),
                    stage: Some(stage_id.into()),
                    restored: false,
                }
            };
            let started = self.options.agents.start(
                task_id,
                agent,
                Some(text),
                Some(terminal),
                session,
                &own_agent,
            );
            started
                .map(|_| self.note_launch(task_id, terminal, agent))
                .map_err(|error| error.to_string())
        };
        match result {
            Ok(()) => Ok(()),
            Err(reason) => self.fail_stage(task_id, stage_id, &reason),
        }
    }

    /// The session of the agent paused in the shell, so the next prompt goes on in its
    /// conversation, as a paste into a running agent would.
    pub fn paused_session(&self, task_id: &str, terminal: &str, agent: &str) -> Option<String> {
        let open = self.options.open_shells.as_ref()?;
        if !open.is_paused(task_id, terminal) {
            return None;
        }
        open.session_of(task_id, terminal, agent)
    }

    pub fn note_launch(&self, task_id: &str, terminal: &str, agent: &str) {
        let launched = Launched {
            agent: agent.into(),
            at: self.now(),
        };
        self.state()
            .launched
            .insert(key(task_id, terminal), launched);
    }

    /// A command whose exit code decides the stage, such as the test command.
    pub fn start_command(&self, task: &Task, stage: &Stage, command: &str) -> StageResult<()> {
        if task.workspace.is_none() {
            self.state().waiting_for_workspace.insert(task.id.clone());
            return Ok(());
        }
        if !self.mark_started(task, stage, Map::new())? {
            return Ok(());
        }
        // The command runs in the same terminal each time, so testing again adds no shell.
        let previous = self.state().command_terminals.get(&task.id).cloned();
        if let Some(previous) = &previous {
            self.options.terminals.close_one(&task.id, previous);
        }
        let terminal = previous.unwrap_or_else(|| {
            let highest = self
                .options
                .terminals
                .list(&task.id)
                .iter()
                .filter_map(|name| name.parse::<u32>().ok())
                .fold(1, u32::max);
            (highest + 1).to_string()
        });
        self.state()
            .command_terminals
            .insert(task.id.clone(), terminal.clone());
        let ran = self
            .command_line(&task.id, &stage.id, command)
            .map_err(|error| error.to_string())
            .and_then(|line| {
                self.options
                    .terminals
                    .run(&task.id, &line, &terminal, DEFAULT_SIZE)
                    .map_err(|error| error.to_string())
            });
        match ran {
            Ok(()) => Ok(()),
            Err(reason) => self.fail_stage(&task.id, &stage.id, &reason),
        }
    }

    /// The command and the report of its result, as one line for the shell of the terminal. On
    /// a POSIX system a script runs them, so a command such as `a; b` stays one unit and a
    /// failing `stage pass` does not run `stage fail` too. PowerShell 5.1 has no && or ||, so on
    /// Windows the line asks $? instead.
    fn command_line(
        &self,
        task_id: &str,
        stage_id: &str,
        command: &str,
    ) -> std::io::Result<String> {
        let pass = format!("natamux stage pass {task_id} --stage {stage_id}");
        let fail = format!("natamux stage fail {task_id} --stage {stage_id}");
        if self.options.windows {
            return Ok(format!("{command}; if ($?) {{ {pass} }} else {{ {fail} }}"));
        }
        std::fs::create_dir_all(self.data_dir())?;
        let script = self.data_dir().join(format!("stage-command-{task_id}.sh"));
        let text = [
            "if (",
            command,
            "); then",
            &format!("  {pass}"),
            "else",
            &format!("  {fail}"),
            "fi",
            "",
        ]
        .join("\n");
        write_private(&script, &text)?;
        Ok(format!(
            "sh {}",
            quote_argument(&script.to_string_lossy(), Platform::Posix)
        ))
    }

    pub fn fail_stage(&self, task_id: &str, stage: &str, reason: &str) -> StageResult<()> {
        self.apply_system(
            task_id,
            "stage.check-failed",
            json!({ "stage": stage, "reasons": [reason], "final": true }),
        )?;
        Ok(())
    }
}

/// Writes a file only the user can read, as Node's `mode: 0o600` (a new file only).
pub fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(text.as_bytes())
}
