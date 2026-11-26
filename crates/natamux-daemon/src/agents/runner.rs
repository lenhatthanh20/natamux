//! `agent-runner.ts`: starts an agent in a terminal of a task, as if the user typed the command.
//! Its hooks then move the task from state to state (part 4).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use natamux_agents::custom::{agent_kind, custom_agent};
use natamux_agents::hook_files::{WorkspaceHooks, prepare_workspace};
use natamux_agents::launch::{
    LAST_SESSION, LaunchOptions, Platform, PromptMode, default_program, is_launchable,
    launch_command, prompt_mode, quote_argument, takes_model,
};
use natamux_agents::launch_ask::{ask_flags, with_ask_env};
use natamux_agents::trust::{TrustPaths, trust_workspace};
use natamux_core::js::is_js_space;
use natamux_core::tasks::dependencies::waiting_on;
use natamux_proto::agents::model::{is_model_name, model_of};
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::text::plain_text;
use natamux_stages::issue::{ISSUE_NOTE, fence_of, imported_url};
use natamux_stages::prompts::{arg_values, fill};

use crate::agents::errors::AgentError;
use crate::agents::hook_config_watch::HookConfigWatch;
use crate::agents::launch_guard::{LaunchGate, drop_trust, guard, path_prefix, with_verify};
use crate::agents::launch_policy_check::{StartWords, args_variable, check_words};
use crate::agents::pasted_prompt::PastedPrompts;
use crate::projects::repo_trust_worktree::TrustedOf;
use crate::shared::jobs::system_clock;
use crate::stages::launcher::write_private;
use crate::tasks::service::TaskService;
use crate::terminals::isolation::Isolation;
use crate::terminals::limits::MAIN_TERMINAL;
use crate::terminals::service::TerminalService;
use crate::terminals::types::DEFAULT_SIZE;
use crate::workspaces::setup_script::WORKSPACE_WARNING_EVENT;

#[path = "runner_isolation.rs"]
mod runner_isolation;

/// On Windows the program of a terminal is always the shell, so the end of an agent cannot be
/// seen there. Its launch line sets these window titles, which reach the terminal as escape
/// sequences: the first as the agent starts, the second once it has ended.
pub const AGENT_TITLE: &str = "natamux-agent";
pub const AGENT_EXITED_TITLE: &str = "natamux-agent-exited";
/// Set when the agent ends with an error, on every platform, so an agent that ends right after
/// its start (no login, a bad flag) is seen in its output.
pub const AGENT_FAILED_TITLE: &str = "natamux-agent-failed";

/// What the POSIX launch script adds after the agent's command line.
pub fn failed_mark() -> String {
    format!(" || printf '\\033]2;{AGENT_FAILED_TITLE}\\007'")
}

/// JS `text.replace(/\s+/g, ' ').trim()`.
fn one_line(text: &str) -> String {
    text.split(is_js_space)
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The words of a stage's `args`, each quoted, so no shell sign in them runs (S-11). A quoted
/// value with spaces in the settings becomes several words. Placeholders are filled in each word
/// after the split, so a value stays one word.
pub fn quoted_words(args: &str, values: &[(&str, &str)], platform: Platform) -> String {
    args.split_ascii_whitespace()
        .map(|word| quote_argument(&fill(word, values), platform))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The task's model for this start: only for the task's agent (or the agent picked for a task
/// without one), a built-in kind that takes `--model` (a custom agent counts by its base), and a
/// stage whose own words name no model, since the stage's `--model` wins.
pub fn task_model<'a>(task: &'a Task, agent: &str, stage_args: Option<&str>) -> Option<&'a str> {
    let model = task.model.as_deref().filter(|model| is_model_name(model))?;
    let its_agent = task.agent.as_deref().is_none_or(|own| own == agent);
    let kind = agent_kind(agent);
    let built_in = LAUNCHABLE_AGENTS.contains(&kind.as_str()) && takes_model(&kind);
    (its_agent && built_in && model_of(stage_args).is_none()).then_some(model)
}

/// The first prompt: the task title, then its description. On one line, because the shell runs
/// the command at the first line end. An imported issue's title and text go together inside a
/// fence longer than any run of backticks in them, so the text cannot end the fence.
pub fn first_prompt(task: &Task) -> String {
    let text = [one_line(&task.title), one_line(&task.description)]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" - ");
    if imported_url(&task.description).is_some() {
        let fence = fence_of(&text);
        return format!("{ISSUE_NOTE} {fence} {text} {fence}");
    }
    text
}

/// A stage with its own agent or words names itself, so the task keeps its agent. So does an
/// agent `restored` in its shell after a restart.
#[derive(Clone, Debug, Default)]
pub struct OwnAgent {
    pub args: Option<String>,
    pub stage: Option<String>,
    pub restored: bool,
}

/// One start, for those who watch the terminal of an agent.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentLaunch {
    pub task_id: String,
    pub terminal: String,
    pub agent: String,
    pub program: String,
    /// The session the agent went on with.
    pub session: Option<String>,
    /// It came back in its shell with no prompt, so it waits at its prompt.
    pub restored: bool,
    /// The task's model passed as `--model`, if any.
    pub model: Option<String>,
}

pub type OnLaunch = Arc<dyn Fn(&AgentLaunch) + Send + Sync>;
/// The permission mode of the agents of a repository, read at each start.
pub type PermissionModeOf = Arc<dyn Fn(Option<&str>) -> String + Send + Sync>;

pub struct AgentRunnerOptions {
    pub tasks: TaskService,
    pub terminals: TerminalService,
    /// Claude Code settings with the hooks that report to this daemon.
    pub claude_settings_path: PathBuf,
    /// Gemini CLI settings with its hooks. Default: next to Claude's.
    pub gemini_settings_path: Option<PathBuf>,
    /// The hook files that only the daemon writes into the workspace.
    pub workspace_hooks: WorkspaceHooks,
    /// The command of each agent (NATAMUX_<AGENT>_COMMAND), and words to add to its command line
    /// (NATAMUX_<AGENT>_ARGS), such as --model sonnet for Claude Code.
    pub programs: HashMap<String, String>,
    pub extra_args: HashMap<String, String>,
    pub windows: bool,
    /// Where the launch scripts go. Default: the folder of the settings file.
    pub launch_dir: Option<PathBuf>,
    /// The folder of the app's `natamux` command (NATAMUX_BIN_DIR), first on the agent's PATH.
    pub bin_dir: Option<String>,
    pub on_launch: Option<OnLaunch>,
    /// Where the agents keep their trusted folders.
    pub trust_paths: TrustPaths,
    pub permission_mode_of: PermissionModeOf,
    /// Whether the project is approved as the task's worktree is now. Without it the agent gets
    /// no auto mode, no trust of the folder and no stage words: the repo's own config would run.
    pub trusted_of: TrustedOf,
}

pub struct AgentRunner {
    options: AgentRunnerOptions,
    programs: HashMap<String, String>,
    extra_args: HashMap<String, String>,
    gemini_settings_path: PathBuf,
    launch_dir: PathBuf,
    /// First prompts of agents that take none on their command line.
    pasted: Arc<PastedPrompts>,
    watch: HookConfigWatch,
    isolation: OnceLock<Arc<Isolation>>,
}

impl AgentRunner {
    pub fn new(options: AgentRunnerOptions) -> AgentRunner {
        let settings_dir = options
            .claude_settings_path
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf);
        let mut programs = HashMap::new();
        for agent in LAUNCHABLE_AGENTS {
            let given = options
                .programs
                .get(agent)
                .map(|program| program.trim())
                .filter(|program| !program.is_empty());
            let program = given.map_or_else(|| default_program(agent).to_string(), str::to_string);
            programs.insert(agent.to_string(), program);
        }
        let extra_args = options
            .extra_args
            .iter()
            .filter(|(agent, _)| is_launchable(agent))
            .map(|(agent, words)| (agent.clone(), words.trim().to_string()))
            .collect();
        let terminals = options.terminals.clone();
        let fronts = options.terminals.clone();
        let pasted = PastedPrompts::new(
            system_clock(),
            Arc::new(move |task_id: &str, terminal: &str, text: &str| {
                // The terminal may have closed since the start: then there is no one to tell.
                let _ = terminals.paste(task_id, text, terminal);
            }),
            Arc::new(move |task_id: &str, terminal: &str| fronts.foreground(task_id, terminal)),
        );
        AgentRunner {
            pasted,
            watch: HookConfigWatch::default(),
            isolation: OnceLock::new(),
            gemini_settings_path: options
                .gemini_settings_path
                .clone()
                .unwrap_or_else(|| settings_dir.join("gemini-settings.json")),
            launch_dir: options.launch_dir.clone().unwrap_or(settings_dir),
            programs,
            extra_args,
            options,
        }
    }

    /// Output of a terminal: a held first prompt waits until the agent's screen settled.
    pub fn on_output(&self, task_id: &str, terminal: &str, data: &[u8]) {
        self.pasted.on_output(task_id, terminal, data);
    }

    /// The first prompts that wait to be pasted.
    pub fn pasted_prompts(&self) -> &Arc<PastedPrompts> {
        &self.pasted
    }

    /// Where the agents keep their trusted folders.
    pub fn trust_paths(&self) -> &TrustPaths {
        &self.options.trust_paths
    }

    /// The program that starts an agent, as the shell looks it up. A custom agent's is its command.
    pub fn program_of(&self, agent: &str) -> String {
        if let Some(custom) = custom_agent(agent) {
            return custom.command;
        }
        self.programs
            .get(agent)
            .cloned()
            .unwrap_or_else(|| default_program(agent).to_string())
    }

    /// Starts the agent in a terminal of the task: the first by default, another for a stage
    /// that runs beside the first one. With a session, the agent goes on with it and gets only
    /// the prompt given, if any.
    pub fn start(
        &self,
        task_id: &str,
        agent: &str,
        prompt: Option<&str>,
        terminal: Option<&str>,
        session: Option<&str>,
        own: &OwnAgent,
    ) -> Result<Task, AgentError> {
        if !is_launchable(agent) {
            return Err(AgentError::Unknown(agent.into()));
        }
        self.check_isolation(agent)?;
        let tasks = &self.options.tasks;
        let task = tasks.get_task(task_id)?;
        let waiting = waiting_on(&task, &tasks.list_tasks(None)?);
        if !waiting.is_empty() {
            return Err(AgentError::Waiting {
                task_id: task_id.into(),
                waiting,
            });
        }
        let program = self.program_of(agent);
        let (trusted_of, paths) = (&self.options.trusted_of, &self.options.trust_paths);
        let gate = guard(trusted_of, &task, &agent_kind(agent), paths)?;
        let (command, pasted) = self.command_of(&task, agent, prompt, session, own, &gate)?;
        let gate = gate.recheck(trusted_of, &task, paths)?;
        let shell = terminal.unwrap_or(MAIN_TERMINAL);
        let check = gate.verify_digest().map(|digest| (task_id, digest));
        let line = self.launch_line(task_id, &command, terminal, check)?;
        let gate = gate.recheck(trusted_of, &task, paths)?;
        // The agent that ran here before is over.
        self.agent_ended(task_id, shell);
        self.options
            .terminals
            .run(task_id, &line, shell, DEFAULT_SIZE)?;
        self.watch.launched(&task, shell, gate.state.digest());
        gate.log_untrusted(tasks, &task, &*self.options.permission_mode_of);
        if let Some(text) = pasted {
            self.pasted.hold(task_id, shell, &text);
        }
        if let Some(on_launch) = &self.options.on_launch {
            let resumed = session.filter(|session| *session != LAST_SESSION);
            on_launch(&AgentLaunch {
                task_id: task_id.into(),
                terminal: shell.into(),
                agent: agent.into(),
                program: program.clone(),
                session: resumed.map(str::to_string),
                restored: own.restored,
                model: self.model_for(&task, agent, own, gate.trusted()),
            });
        }
        let restored_in = own.restored.then_some(shell);
        let kept = session.filter(|session| !session.is_empty() && *session != LAST_SESSION);
        Ok(tasks.record_agent_launch(
            task_id,
            agent,
            Some(&program),
            own.stage.as_deref(),
            restored_in,
            (shell, kept),
        )?)
    }

    /// The agent of a shell ended: its config changed meanwhile goes into the task's log, and
    /// Strict mode takes back Cursor's and Copilot's trust of the worktree.
    pub fn agent_ended(&self, task_id: &str, terminal: &str) {
        let trusted_of = &self.options.trusted_of;
        let Some(message) = self.watch.ended(task_id, terminal, trusted_of) else {
            return;
        };
        let tasks = &self.options.tasks;
        let payload = serde_json::json!({ "message": message });
        let logged = tasks.record_event(task_id, WORKSPACE_WARNING_EVENT, payload, "system");
        if let Err(error) = logged {
            eprintln!("The config warning of {task_id} is not logged: {error}");
        }
        if natamux_core::security::mode().is_strict()
            && let Ok(task) = tasks.get_task(task_id)
        {
            drop_trust(&task, &self.options.trust_paths);
        }
    }

    /// The task's model for this start. Stage words of an untrusted project are left out, so a
    /// `--model` in them does not count.
    fn model_for(&self, task: &Task, agent: &str, own: &OwnAgent, trusted: bool) -> Option<String> {
        let stage_args = own.args.as_deref().filter(|_| trusted);
        task_model(task, agent, stage_args).map(str::to_string)
    }

    /// The word check of `launch_policy_check`, before anything is written. Gives whether the
    /// project's permission mode is `auto`, read at each start so a saved change counts.
    fn checked_mode(
        &self,
        task: &Task,
        kind: &str,
        gate: &LaunchGate,
        [command, model, stage_words]: [Option<&str>; 3],
    ) -> Result<bool, AgentError> {
        let auto_mode = (self.options.permission_mode_of)(task.repo.as_deref()) == "auto";
        let variable = args_variable(kind);
        let words = StartWords {
            kind,
            // An untrusted worktree starts without auto flags, so it gets the ask check.
            ask: !(auto_mode && gate.trusted()),
            strict: gate.strict(),
            sources: [
                ("agent command", command),
                (
                    variable.as_str(),
                    self.extra_args.get(kind).map(String::as_str),
                ),
                ("model", model),
                ("stage args", stage_words),
            ],
        };
        check_words(&self.options.tasks, &task.id, &words)?;
        Ok(auto_mode)
    }

    /// The command line of a start, with the agent's hook file and trust in place first, and
    /// the prompt to paste once the agent is up, for an agent that takes none on the line.
    fn command_of(
        &self,
        task: &Task,
        agent: &str,
        prompt: Option<&str>,
        session: Option<&str>,
        own: &OwnAgent,
        gate: &LaunchGate,
    ) -> Result<(String, Option<String>), AgentError> {
        let (trusted, strict) = (gate.trusted(), gate.strict());
        // A custom agent starts as its base, with its own program.
        let kind = agent_kind(agent);
        let stage_words = own
            .args
            .as_deref()
            .filter(|_| trusted)
            .map(|args| quoted_words(args, &arg_values(task), self.platform()));
        let model = self
            .model_for(task, agent, own, trusted)
            .map(|model| format!("--model {}", quote_argument(&model, self.platform())));
        let program = self.program_of(agent);
        let words = [
            Some(program.as_str()),
            model.as_deref(),
            stage_words.as_deref(),
        ];
        let auto_mode = self.checked_mode(task, &kind, gate, words)?;
        let mut workspace_hooks = false;
        if let Some(workspace) = &task.workspace {
            workspace_hooks =
                prepare_workspace(&kind, Path::new(workspace), self.options.workspace_hooks);
            // Strict mode never writes Cursor's or Copilot's trust of a path.
            if trusted && !strict {
                trust_workspace(&kind, workspace, &self.options.trust_paths);
            }
        }
        // Strict mode: the agent's ask mode, before the user's own words.
        let ask = strict.then(|| ask_flags(&kind).join(" "));
        let extra: Vec<&str> = [
            ask.as_deref(),
            self.extra_args.get(&kind).map(String::as_str),
            model.as_deref(),
            stage_words.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|words| !words.is_empty())
        .collect();
        let text = start_text(task, session, prompt);
        let (text, pasted) = match prompt_mode(&kind) {
            PromptMode::Pasted if !text.is_empty() => (String::new(), Some(text)),
            _ => (text, None),
        };
        let command = launch_command(
            &kind,
            &LaunchOptions {
                program,
                extra_args: extra.join(" "),
                prompt: text,
                session: session.map(str::to_string),
                platform: self.platform(),
                claude_settings_path: self.options.claude_settings_path.to_string_lossy().into(),
                gemini_settings_path: self.gemini_settings_path.to_string_lossy().into(),
                auto: !strict && trusted && auto_mode,
                workspace: task.workspace.clone(),
                trust: trusted,
                workspace_hooks,
                bypass_hook_trust: gate.bypass_hook_trust(),
            },
        );
        if strict {
            return Ok((with_ask_env(&kind, &command, self.platform()), pasted));
        }
        Ok((command, pasted))
    }

    /// What is typed into the terminal. The terminal echoes it, so the long command (settings
    /// path, prompt) goes into a small script and only a short line to run it is typed. The
    /// script is sourced, not run by `sh`, so the user's own shell (aliases, PATH) still starts
    /// the agent. PowerShell may refuse scripts by policy, so Windows types the command itself,
    /// between the window titles that tell when the agent starts and ends. A login shell's
    /// profile can push the app's bin folder behind an older global `natamux`; the PATH prefix
    /// puts it first again, for the agent and its hooks only.
    fn launch_line(
        &self,
        task_id: &str,
        command: &str,
        terminal: Option<&str>,
        check: Option<(&str, &str)>,
    ) -> Result<String, AgentError> {
        let platform = self.platform();
        let path = path_prefix(self.options.bin_dir.as_deref(), platform);
        if self.options.windows {
            // Strict: the check puts the bin folder first in PATH, for the rest of the session.
            let command = with_verify(command, check, &path, platform);
            let title = |text: &str| format!("$Host.UI.RawUI.WindowTitle = '{text}'");
            return Ok(format!(
                "{}; {command}; if ($LASTEXITCODE -ne 0) {{ {} }}; {}",
                title(AGENT_TITLE),
                title(AGENT_FAILED_TITLE),
                title(AGENT_EXITED_TITLE)
            ));
        }
        let io = |error: std::io::Error| {
            AgentError::Task(crate::tasks::errors::TaskError::Failed(error.to_string()))
        };
        let launch_dir = self.launch_dir_of(task_id);
        std::fs::create_dir_all(&launch_dir).map_err(io)?;
        let suffix = match terminal {
            None | Some("1") => String::new(),
            Some(terminal) => format!("-{terminal}"),
        };
        let script = launch_dir.join(format!("agent-launch-{task_id}{suffix}.sh"));
        let command = with_verify(&format!("{path}{command}"), check, &path, platform);
        // `||` and printf, not `$?`, which fish does not have.
        write_private(&script, &format!("{command}{}\n", failed_mark())).map_err(io)?;
        Ok(format!(
            ". {}",
            quote_argument(&script.to_string_lossy(), platform)
        ))
    }
}

/// The prompt of a start: the task's first prompt when none is given, none for a bare resume.
fn start_text(task: &Task, session: Option<&str>, prompt: Option<&str>) -> String {
    if session.is_some() && prompt.is_none() {
        return String::new();
    }
    let text = prompt.map_or_else(|| first_prompt(task), str::to_string);
    one_line(&plain_text(&text))
}
