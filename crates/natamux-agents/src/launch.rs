//! The command lines of `packages/daemon/src/agents/agent-launch.ts`: how each agent is started,
//! run once, and told apart in a shell.

use natamux_core::js::{is_js_space, js_trim};
use natamux_core::json::write_string;
use natamux_proto::terminals::list::SHELL_PROGRAMS;

use crate::hook_files::{CODEX_EVENTS, codex_hook_value};

pub use natamux_proto::agents::model::takes_model;
pub use natamux_proto::agents::start::{DEFAULT_PROGRAMS, LAUNCHABLE_AGENTS};

/// What the hooks run. `natamux hook` does nothing outside a Natamux terminal (no token).
pub const HOOK: &str = "natamux hook";

/// The shell of the terminal: PowerShell on Windows, a POSIX shell elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Posix,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Posix
        }
    }
}

/// A built-in agent or one of the user's custom agents.
pub fn is_launchable(agent: &str) -> bool {
    LAUNCHABLE_AGENTS.contains(&agent) || crate::custom::custom_agent(agent).is_some()
}

/// The default program of an agent; empty for a name that is not launchable.
pub fn default_program(agent: &str) -> &'static str {
    DEFAULT_PROGRAMS
        .iter()
        .find(|(name, _)| *name == agent)
        .map_or("", |(_, program)| program)
}

/// Other names an agent is installed under. Cursor's installer adds `agent` next to
/// `cursor-agent`; Kimi Code installs `kimi-code` next to `kimi`.
fn other_programs(agent: &str) -> &'static [&'static str] {
    match agent {
        "cursor" => &["agent"],
        "kimi" => &["kimi-code"],
        _ => &[],
    }
}

/// How an agent gets its first prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptMode {
    /// On its command line, and it stays open afterwards.
    Argument,
    /// It takes none on the line (or a prompt there makes it run once and end), so the prompt is
    /// pasted into its terminal once it is up.
    Pasted,
}

/// Agents that start without a prompt: Amp, Kimi, Aider, Crush, Muse have no flag for it; a
/// prompt on Cline's line runs it headless; Goose's and Kiro's flags for it are not documented
/// to keep the screen open. Orca pastes for all of them too.
const PASTED_PROMPT_AGENTS: [&str; 8] = [
    "amp", "kimi", "goose", "aider", "crush", "cline", "kiro", "muse",
];

pub fn prompt_mode(agent: &str) -> PromptMode {
    if PASTED_PROMPT_AGENTS.contains(&agent) {
        PromptMode::Pasted
    } else {
        PromptMode::Argument
    }
}

/// Programs that run the script of an agent: one installed with npm runs as `node <path>/codex`.
fn is_runtime(name: &str) -> bool {
    ["node", "bun", "deno"].contains(&name) || SHELL_PROGRAMS.contains(&name)
}

/// `/usr/lib/node_modules/@openai/codex/bin/codex.js` names the program `codex`.
fn program_name(word: &str) -> &str {
    let trimmed = word.trim_end_matches(['/']);
    let base = trimmed.rsplit('/').next().unwrap_or("");
    let base = if cfg!(windows) {
        base.rsplit('\\').next().unwrap_or("")
    } else {
        base
    };
    for suffix in [".js", ".cjs", ".mjs", ".exe"] {
        if let Some(stem) = base.strip_suffix(suffix) {
            return stem;
        }
    }
    base
}

/// The agent a command line runs, by its program: the one configured for an agent or the default.
/// After a runtime, the script counts.
pub fn agent_of_command(line: &str, program_of: impl Fn(&str) -> String) -> Option<&'static str> {
    let words: Vec<&str> = js_trim(line)
        .split(is_js_space)
        .filter(|word| !word.is_empty())
        .collect();
    let first = program_name(words.first().copied().unwrap_or(""));
    let script = words.iter().skip(1).find(|word| !word.starts_with('-'));
    let name = match script {
        Some(script) if is_runtime(first) => program_name(script),
        _ => first,
    };
    LAUNCHABLE_AGENTS.into_iter().find(|agent| {
        let configured = program_of(agent);
        let configured = js_trim(&configured)
            .split(is_js_space)
            .next()
            .unwrap_or("")
            .to_string();
        let mut programs = vec![default_program(agent).to_string()];
        programs.extend(other_programs(agent).iter().map(|each| (*each).to_string()));
        programs.push(configured);
        programs.iter().any(|program| program_name(program) == name)
    })
}

/// The flags of the `auto` permission mode. Only Claude Code checks each action in its auto mode;
/// the others have no such check, so their closest flag lets them run (almost) anything. Goose
/// has no flag for its mode, so its arm in `launch_command` sets `GOOSE_MODE=auto` instead.
pub const AUTO_FLAGS: [(&str, &[&str]); 15] = [
    ("claude-code", &["--permission-mode", "auto"]),
    // Codex dropped `--full-auto`. The first two flags mean the same and exist in old and new
    // versions. Its `workspace-write` sandbox blocks the network, and with it the daemon on
    // loopback, so the `natamux` commands of a stage could not report.
    (
        "codex",
        &[
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "never",
            "-c",
            "sandbox_workspace_write.network_access=true",
        ],
    ),
    ("gemini", &["--approval-mode", "yolo"]),
    ("copilot", &["--allow-all-tools"]),
    ("cursor", &["--force"]),
    ("opencode", &["--auto"]),
    ("qwen", &["--approval-mode", "yolo"]),
    // Documented for `droid exec`; the interactive mode takes it too.
    ("droid", &["--auto", "high"]),
    ("amp", &["--dangerously-allow-all"]),
    ("kimi", &["--yolo"]),
    ("aider", &["--yes-always"]),
    ("crush", &["--yolo"]),
    ("cline", &["--auto-approve", "true"]),
    ("kiro", &["--trust-all-tools"]),
    ("muse", &["--yolo"]),
];

pub fn auto_flags(agent: &str) -> &'static [&'static str] {
    AUTO_FLAGS
        .iter()
        .find(|(name, _)| *name == agent)
        .map_or(&[], |(_, flags)| flags)
}

/// Quotes one argument for the shell of the terminal.
pub fn quote_argument(value: &str, platform: Platform) -> String {
    match platform {
        // PowerShell also ends a quoted string at the typographic single quotes, which titles of
        // imported issues often hold. Doubling each one keeps it inside the string.
        Platform::Windows => {
            let mut out = String::from("'");
            for c in value.chars() {
                out.push(c);
                if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
                    out.push(c);
                }
            }
            out.push('\'');
            out
        }
        Platform::Posix => format!("'{}'", value.replace('\'', "'\\''")),
    }
}

/// A prompt that starts with `-` (an imported issue title) would read as an option of the agent.
fn task_word(prompt: &str) -> String {
    if prompt.starts_with('-') {
        format!("Task: {prompt}")
    } else {
        prompt.to_string()
    }
}

/// The hooks Codex takes for one session as `-c` overrides. They count only with the trust check
/// off for this run: Codex asks to trust every hook it has not seen.
pub fn codex_hook_args() -> Vec<String> {
    CODEX_EVENTS
        .iter()
        .flat_map(|event| {
            [
                "-c".to_string(),
                format!("hooks.{event}={}", codex_hook_value(event)),
            ]
        })
        .collect()
}

/// Codex splits a `-c` key at each dot, so the path goes into a table value. A JSON string is a
/// valid TOML string.
fn codex_trust_arg(workspace: &str) -> String {
    let mut path = String::new();
    write_string(&mut path, workspace);
    format!("projects={{{path}={{trust_level=\"trusted\"}}}}")
}

/// In place of a session id: the newest Claude Code session of the folder. No id looks like it.
pub const LAST_SESSION: &str = "--continue";

#[derive(Clone, Debug)]
pub struct LaunchOptions {
    pub program: String,
    /// Words added to the command line, from `NATAMUX_<AGENT>_ARGS`.
    pub extra_args: String,
    pub prompt: String,
    /// A session of the agent to go on with, or `LAST_SESSION`.
    pub session: Option<String>,
    pub platform: Platform,
    pub claude_settings_path: String,
    pub gemini_settings_path: String,
    /// Whether the permission mode is `auto`; any other mode adds no flags.
    pub auto: bool,
    /// The task's worktree, which the agent is told to trust. None: no trust is added.
    pub workspace: Option<String>,
    /// Whether the agent is told to trust the folder (Codex's `trust_level`, Gemini's trust
    /// variable). Off for a project whose commands are not approved: its own agent config
    /// (hooks, MCP servers) would run.
    pub trust: bool,
    /// The agent's hook file is in the worktree, so the launch need not pass them.
    pub workspace_hooks: bool,
}

/// The command line that starts an agent in its interactive mode, with its first prompt and the
/// hooks that report to the daemon. Each stays open for the prompts of the next stages.
pub fn launch_command(agent: &str, options: &LaunchOptions) -> String {
    let quote = |value: &str| quote_argument(value, options.platform);
    // The mode comes before the extra words, so words of the user's own can still change it.
    let mode: Vec<String> = if options.auto {
        auto_flags(agent)
            .iter()
            .map(|flag| (*flag).to_string())
            .collect()
    } else {
        Vec::new()
    };
    let extra: Vec<String> = if options.extra_args.is_empty() {
        Vec::new()
    } else {
        vec![options.extra_args.clone()]
    };
    let prompt: Vec<String> = if options.prompt.is_empty() {
        Vec::new()
    } else {
        vec![quote(&task_word(&options.prompt))]
    };
    let session = options.session.as_deref();
    // `--resume <id>`, or the flag of the newest session of the folder.
    let resume = |newest: &str| -> Vec<String> {
        match session {
            None => Vec::new(),
            Some(LAST_SESSION) => vec![newest.to_string()],
            Some(id) => vec!["--resume".to_string(), quote(id)],
        }
    };
    let program = options.program.clone();
    let words: Vec<String> = match agent {
        "claude-code" => {
            let mut words = vec![program];
            // Only without the workspace file: with both, Claude Code runs each hook twice.
            if !options.workspace_hooks {
                words.push("--settings".into());
                words.push(quote(&options.claude_settings_path));
            }
            words.extend(mode);
            words.extend(resume(LAST_SESSION));
            words.extend(extra);
            words.extend(prompt);
            words
        }
        "codex" => codex_words(options, &quote, mode, extra, prompt),
        "gemini" => return gemini_line(options, &quote, mode, extra, prompt, resume("--resume")),
        "copilot" => {
            let mut words = vec![program];
            words.extend(mode);
            words.extend(extra);
            words.extend(resume("--continue"));
            if !prompt.is_empty() {
                words.push("-i".into());
                words.extend(prompt);
            }
            words
        }
        "cursor" => {
            let mut words = vec![program];
            words.extend(mode);
            words.extend(extra);
            words.extend(resume("--continue"));
            words.extend(prompt);
            words
        }
        "opencode" | "qwen" | "droid" => {
            let mut words = vec![program];
            words.extend(mode);
            words.extend(extra);
            words.extend(other_resume(agent, session, &quote));
            if !prompt.is_empty() {
                match agent {
                    "opencode" => words.push("--prompt".into()),
                    // Without -i Qwen Code runs the prompt once and ends.
                    "qwen" => words.push("-i".into()),
                    _ => {}
                }
                words.extend(prompt);
            }
            words
        }
        // Muse reads its subcommand first, and a prompt only once it is up (pasted).
        "muse" => {
            let mut words = vec![program];
            words.extend(other_resume(agent, session, &quote));
            if options.trust {
                words.push("--trust-workspace".into());
            }
            words.extend(mode);
            words.extend(extra);
            words
        }
        // The prompt of these is pasted once they are up.
        "amp" | "kimi" | "goose" | "aider" | "crush" | "cline" | "kiro" => {
            // Goose has no flag for its mode, and a user's config may set `approve`.
            let goose_auto = agent == "goose" && options.auto;
            let mut words = Vec::new();
            if goose_auto {
                words.push(goose_mode("auto", options.platform));
            }
            words.push(program);
            match agent {
                "goose" => words.push("session".into()),
                "kiro" => words.push("chat".into()),
                _ => {}
            }
            words.extend(mode);
            words.extend(extra);
            words.extend(other_resume(agent, session, &quote));
            words
        }
        // Any other CLI (a custom agent on base "other"): the prompt is its last word.
        _ => {
            let mut words = vec![program];
            words.extend(extra);
            words.extend(prompt);
            words
        }
    };
    words.join(" ")
}

/// The resume words of the agents added after the first five: by id, or the newest session of
/// the folder (`LAST_SESSION`). None where the agent has no such flag.
fn other_resume(agent: &str, session: Option<&str>, quote: &dyn Fn(&str) -> String) -> Vec<String> {
    let Some(session) = session else {
        return Vec::new();
    };
    let newest = session == LAST_SESSION;
    let (by_id, last): (&[&str], &[&str]) = match agent {
        "opencode" => (&["--session"], &["--continue"]),
        "qwen" => (&["--resume"], &["--continue"]),
        "droid" => (&["--resume"], &["--resume"]),
        "kimi" => (&["--session"], &["--continue"]),
        "goose" => (&["--resume", "--session-id"], &["--resume"]),
        "aider" => (&[], &["--restore-chat-history"]),
        "crush" => (&["--session"], &["--continue"]),
        "cline" => (&["--id"], &[]),
        "kiro" => (&["--resume-id"], &["--resume"]),
        "muse" => (&["resume"], &[]),
        _ => (&[], &[]),
    };
    let flags = if newest { last } else { by_id };
    let mut words: Vec<String> = flags.iter().map(|word| (*word).to_string()).collect();
    if !newest && !words.is_empty() {
        words.push(quote(session));
    }
    words
}

fn codex_words(
    options: &LaunchOptions,
    quote: &dyn Fn(&str) -> String,
    mode: Vec<String>,
    extra: Vec<String>,
    prompt: Vec<String>,
) -> Vec<String> {
    let session = options.session.as_deref();
    let mut words = vec![options.program.clone()];
    // `codex resume [OPTIONS] [SESSION_ID]` takes the same options as `codex`.
    if session.is_some() {
        words.push("resume".into());
    }
    words.push("--dangerously-bypass-hook-trust".into());
    words.extend(mode);
    if let (true, Some(workspace)) = (options.trust, &options.workspace) {
        words.push("-c".into());
        words.push(quote(&codex_trust_arg(workspace)));
    }
    words.extend(
        codex_hook_args()
            .into_iter()
            .map(|arg| if arg == "-c" { arg } else { quote(&arg) }),
    );
    // Inline, so its output goes into the terminal history. Codex exits when the flag comes
    // twice, so the user's own one is enough.
    if !options
        .extra_args
        .split(is_js_space)
        .any(|word| word.trim_matches('\'') == "--no-alt-screen")
    {
        words.push("--no-alt-screen".into());
    }
    words.extend(extra);
    match session {
        None => {}
        Some(LAST_SESSION) => words.push("--last".into()),
        Some(id) => words.push(quote(id)),
    }
    words.extend(prompt);
    words
}

fn gemini_line(
    options: &LaunchOptions,
    quote: &dyn Fn(&str) -> String,
    mode: Vec<String>,
    extra: Vec<String>,
    prompt: Vec<String>,
    resume: Vec<String>,
) -> String {
    let mut command = vec![options.program.clone()];
    command.extend(mode);
    command.extend(extra);
    // A bare `--resume` is the newest session.
    command.extend(resume);
    if !prompt.is_empty() {
        command.push("-i".into());
        command.extend(prompt);
    }
    let settings = quote(&options.gemini_settings_path);
    // Folder trust is off by default; when the user turned it on, this skips its question. With
    // the hooks in the workspace's settings, the system settings would run each hook twice.
    let system = !options.workspace_hooks;
    let windows = options.platform == Platform::Windows;
    let mut words = Vec::new();
    if options.trust {
        words.push(if windows {
            "$env:GEMINI_CLI_TRUST_WORKSPACE='true';".to_string()
        } else {
            "GEMINI_CLI_TRUST_WORKSPACE=true".to_string()
        });
    }
    if system {
        words.push(if windows {
            format!("$env:GEMINI_CLI_SYSTEM_SETTINGS_PATH={settings};")
        } else {
            format!("GEMINI_CLI_SYSTEM_SETTINGS_PATH={settings}")
        });
    }
    words.push(command.join(" "));
    words.join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneShot {
    pub line: String,
    pub stdin: String,
}

/// The command line that runs an agent once, without its interactive screen, for a short answer
/// such as a commit message: the prompt goes on stdin where the agent reads it there, so a long
/// prompt does not hit the limit of a command line. No hooks, no permission flags: it needs no
/// tools, and must not move the task.
pub fn one_shot_command(agent: &str, program: &str, prompt: &str, platform: Platform) -> OneShot {
    let quote = |value: &str| quote_argument(&task_word(value), platform);
    let (line, stdin) = match agent {
        "claude-code" => (format!("{program} -p"), prompt.to_string()),
        "codex" => (format!("{program} exec -"), prompt.to_string()),
        // Gemini puts the text of -p after what it reads on stdin.
        "gemini" => (
            format!("{program} -p {}", quote("Answer the request above.")),
            prompt.to_string(),
        ),
        "copilot" => (format!("{program} -p {}", quote(prompt)), String::new()),
        "cursor" => (
            format!("{program} -p --output-format text {}", quote(prompt)),
            String::new(),
        ),
        // Qwen Code puts what it reads on stdin before the text of -p, as Gemini does.
        "qwen" => (
            format!("{program} -p {}", quote("Answer the request above.")),
            prompt.to_string(),
        ),
        // Without `--auto`, `droid exec` is read-only.
        "droid" => (format!("{program} exec"), prompt.to_string()),
        // `chat` mode runs no tools, so the draft cannot change files.
        "goose" => (
            format!(
                "{} {program} run -t {}",
                goose_mode("chat", platform),
                quote(prompt)
            ),
            String::new(),
        ),
        "kiro" => (
            format!("{program} chat --no-interactive {}", quote(prompt)),
            String::new(),
        ),
        _ => (program.to_string(), String::new()),
    };
    OneShot { line, stdin }
}

/// Sets Goose's mode for the command after it, in the shell of the terminal.
fn goose_mode(mode: &str, platform: Platform) -> String {
    match platform {
        Platform::Windows => format!("$env:GOOSE_MODE='{mode}';"),
        Platform::Posix => format!("GOOSE_MODE={mode}"),
    }
}

/// Agents with no mode that only answers: their one run is a whole turn with tools, which may
/// change files. Aider's `--message`, Cline's prompt, `kimi --print` (it turns on `--afk`),
/// `crush run` (it approves each permission), `amp -x`, and `opencode run` (every tool allowed by
/// default) do so; Muse's docs do not say that `muse exec` runs no tools. A short answer of
/// theirs comes from Claude Code.
const NO_ONE_SHOT_AGENTS: [&str; 7] =
    ["aider", "cline", "kimi", "crush", "amp", "opencode", "muse"];

pub fn has_one_shot(agent: &str) -> bool {
    LAUNCHABLE_AGENTS.contains(&agent) && !NO_ONE_SHOT_AGENTS.contains(&agent)
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "command_line_tests.rs"]
mod command_line_tests;

#[cfg(test)]
#[path = "launch_trust_tests.rs"]
mod trust_tests;
