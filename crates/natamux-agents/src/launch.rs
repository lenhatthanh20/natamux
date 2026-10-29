//! The command lines of `packages/daemon/src/agents/agent-launch.ts`: how each agent is started,
//! run once, and told apart in a shell.

use natamux_core::js::{is_js_space, js_trim};
use natamux_core::json::write_string;
use natamux_proto::terminals::list::SHELL_PROGRAMS;

use crate::hook_files::{CODEX_EVENTS, codex_hook_value};

pub use natamux_proto::agents::start::LAUNCHABLE_AGENTS;

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

pub fn is_launchable(agent: &str) -> bool {
    LAUNCHABLE_AGENTS.contains(&agent)
}

/// The command each agent is started with when no other is configured.
pub const DEFAULT_PROGRAMS: [(&str, &str); 5] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("gemini", "gemini"),
    ("copilot", "copilot"),
    ("cursor", "cursor-agent"),
];

/// The default program of an agent; empty for a name that is not launchable.
pub fn default_program(agent: &str) -> &'static str {
    DEFAULT_PROGRAMS
        .iter()
        .find(|(name, _)| *name == agent)
        .map_or("", |(_, program)| program)
}

/// Other names an agent is installed under. Cursor's installer adds `agent` next to
/// `cursor-agent`.
fn other_programs(agent: &str) -> &'static [&'static str] {
    if agent == "cursor" { &["agent"] } else { &[] }
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
/// the others have no such check, so their closest flag lets them run (almost) anything.
pub const AUTO_FLAGS: [(&str, &[&str]); 5] = [
    ("claude-code", &["--permission-mode", "auto"]),
    // Codex dropped `--full-auto`. These two flags mean the same and exist in old and new versions.
    (
        "codex",
        &[
            "--sandbox",
            "workspace-write",
            "--ask-for-approval",
            "never",
        ],
    ),
    ("gemini", &["--approval-mode", "yolo"]),
    ("copilot", &["--allow-all-tools"]),
    ("cursor", &["--force"]),
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
        vec![quote(&options.prompt)]
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
        _ => vec![program],
    };
    words.join(" ")
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
    let quote = |value: &str| quote_argument(value, platform);
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
        _ => (program.to_string(), String::new()),
    };
    OneShot { line, stdin }
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
