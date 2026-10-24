use super::*;
use crate::hook_files::{copilot_hook_file, cursor_hook_file, gemini_hook_settings};
use natamux_core::json::to_js_string;
use serde_json::json;

const POSIX: Platform = Platform::Posix;
const WIN: Platform = Platform::Windows;

#[test]
fn runs_each_agent_once_with_the_prompt_on_stdin_or_as_a_word() {
    for (agent, line, stdin) in [
        ("claude-code", "claude -p", "Write it"),
        ("codex", "codex exec -", "Write it"),
        (
            "gemini",
            "gemini -p 'Answer the request above.'",
            "Write it",
        ),
        ("copilot", "copilot -p 'Write it'", ""),
        (
            "cursor",
            "cursor-agent -p --output-format text 'Write it'",
            "",
        ),
    ] {
        let got = one_shot_command(agent, default_program(agent), "Write it", POSIX);
        assert_eq!(
            got,
            OneShot {
                line: line.into(),
                stdin: stdin.into()
            },
            "{agent}"
        );
    }
}

#[test]
fn starts_each_agent_with_no_permission_flags_hooks_or_settings() {
    for agent in LAUNCHABLE_AGENTS {
        let got = one_shot_command(agent, default_program(agent), "Write it", POSIX);
        let words: Vec<&str> = got.line.split(' ').collect();
        let mut flags: Vec<&str> = auto_flags(agent).to_vec();
        flags.extend(["--settings", "natamux hook", "-i"]);
        for flag in flags {
            assert!(!words.contains(&flag), "{agent} {flag}");
        }
        assert!(!got.line.contains("GEMINI_CLI_SYSTEM_SETTINGS_PATH"));
    }
}

#[cfg(unix)]
#[test]
fn gives_a_prompt_with_quotes_dollar_backticks_newlines_semicolons_and_and_as_one_word_unchanged() {
    let prompt = "it's \"$HOME\" `id` $(id)\nnext; echo x && echo y";
    let got = one_shot_command("copilot", "printf %s", prompt, POSIX);
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", &got.line.replace(" -p ", " ")])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(output.stdout).unwrap(), prompt);
}

#[test]
fn keeps_the_words_of_a_configured_program_and_quotes_the_prompt_for_the_shell() {
    assert_eq!(
        one_shot_command("copilot", "node /opt/copilot.js", "it's", POSIX).line,
        r"node /opt/copilot.js -p 'it'\''s'"
    );
    assert_eq!(
        one_shot_command("cursor", "agent", "it's", WIN).line,
        "agent -p --output-format text 'it''s'"
    );
}

fn defaults(agent: &str) -> String {
    default_program(agent).to_string()
}

#[test]
fn finds_the_agent_of_a_command_line() {
    for (line, agent) in [
        ("claude --settings /data/claude.json Fix it", "claude-code"),
        ("/Users/me/.local/bin/claude", "claude-code"),
        ("codex --sandbox workspace-write", "codex"),
        // Installed with npm: the program is Node.js, the agent is its script.
        ("node /usr/local/bin/codex", "codex"),
        (
            "/usr/local/bin/node --no-warnings /opt/lib/node_modules/@openai/codex/bin/codex.js",
            "codex",
        ),
        ("node /opt/homebrew/bin/gemini -i Fix it", "gemini"),
        ("node /opt/homebrew/bin/copilot", "copilot"),
        (
            "/bin/bash /Users/me/.local/bin/cursor-agent --force",
            "cursor",
        ),
        ("codex --model x", "codex"),
        ("claude -p hi", "claude-code"),
        ("/usr/local/bin/codex", "codex"),
        ("cursor-agent", "cursor"),
        // Cursor's own launcher keeps the name it was called by, `agent`, in front of Node.js.
        (
            "/Users/me/.local/bin/agent --use-system-ca /Users/me/.local/share/cursor-agent/versions/1/index.js",
            "cursor",
        ),
        ("agent", "cursor"),
    ] {
        assert_eq!(agent_of_command(line, defaults), Some(agent), "{line}");
    }
}

#[test]
fn finds_no_agent_in_a_command_line_that_is_none() {
    for line in [
        "vim codex.md",
        "node server.js",
        "bash ./build.sh",
        "sleep 60",
        "",
        // Only the name of an agent counts, not a name that holds one.
        "claude-foo",
        "my-codex",
        // Node.js itself, or a tool that runs on it.
        "node",
        "node /usr/local/bin/pnpm dev",
        "node /usr/local/bin/npm run dev",
        "node /home/u/codex-notes/server.js",
        "bash",
        "htop",
        "less codex.log",
        // An agent on another machine is not in this shell.
        "ssh dev codex",
        "docker exec -it box claude",
    ] {
        assert_eq!(agent_of_command(line, defaults), None, "{line}");
    }
}

#[test]
fn knows_an_agent_by_the_program_configured_for_it() {
    let program_of = |agent: &str| {
        if agent == "claude-code" {
            "/opt/tools/my-claude --verbose".to_string()
        } else {
            defaults(agent)
        }
    };
    assert_eq!(
        agent_of_command("/opt/tools/my-claude --verbose", program_of),
        Some("claude-code")
    );
    assert_eq!(
        agent_of_command("node /opt/tools/my-claude", program_of),
        Some("claude-code")
    );
    // The default name still counts, for an agent the user starts by hand.
    assert_eq!(agent_of_command("claude", program_of), Some("claude-code"));
}

/// The values after `command="`, `"command":"`, `bash` and `powershell` in a text, as
/// `(?:command|bash|powershell)"?[:=]"([^"]+)"`.
fn commands_of(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for key in ["command", "bash", "powershell"] {
        for (at, _) in text.match_indices(key) {
            let rest = &text[at + key.len()..];
            let rest = rest.strip_prefix('"').unwrap_or(rest);
            let Some(rest) = rest.strip_prefix([':', '=']) else {
                continue;
            };
            let Some(rest) = rest.strip_prefix('"') else {
                continue;
            };
            if let Some(end) = rest.find('"').filter(|end| *end > 0) {
                found.push(rest[..end].to_string());
            }
        }
    }
    found
}

#[test]
fn sends_every_event_natamux_reads_to_natamux_hook_agent() {
    assert_eq!(
        codex_hook_args().iter().filter(|arg| *arg != "-c").count(),
        6
    );
    let gemini = gemini_hook_settings();
    let events: Vec<&String> = gemini["hooks"].as_object().unwrap().keys().collect();
    assert_eq!(
        events,
        [
            "BeforeAgent",
            "BeforeTool",
            "AfterTool",
            "Notification",
            "AfterAgent"
        ]
    );
    let copilot = copilot_hook_file();
    assert_eq!(
        copilot["hooks"]["agentStop"],
        json!([{
            "type": "command",
            "bash": "natamux hook copilot agentStop",
            "powershell": "natamux hook copilot agentStop",
            "timeoutSec": 5,
        }])
    );
    // permissionRequest fires for each tool call, also one --allow-all-tools approves at once.
    assert!(copilot["hooks"].get("notification").is_some());
    assert!(copilot["hooks"].get("permissionRequest").is_none());
    let cursor = cursor_hook_file();
    assert_eq!(cursor["version"], json!(1));
    assert_eq!(
        cursor["hooks"]["stop"],
        json!([{ "command": "natamux hook cursor", "timeout": 5 }])
    );
}

#[test]
fn keeps_gemini_inline_its_system_settings_win_over_the_user_settings() {
    assert_eq!(
        gemini_hook_settings()["ui"],
        json!({ "useAlternateBuffer": false })
    );
}

#[test]
fn starts_every_hook_of_the_four_agents_with_natamux_hook_agent_the_fast_path() {
    let agents = [
        ("codex", codex_hook_args().join(" ")),
        ("gemini", to_js_string(&gemini_hook_settings())),
        ("copilot", to_js_string(&copilot_hook_file())),
        ("cursor", to_js_string(&cursor_hook_file())),
    ];
    for (agent, text) in agents {
        let commands = commands_of(&text);
        assert!(!commands.is_empty(), "{agent}");
        for command in commands {
            assert!(
                command.starts_with(&format!("natamux hook {agent}")),
                "{agent}"
            );
        }
    }
}

#[test]
fn doubles_the_typographic_single_quotes_for_powershell_too() {
    assert_eq!(
        quote_argument("Don\u{2019}t crash; Remove-Item ~\\src", WIN),
        "'Don\u{2019}\u{2019}t crash; Remove-Item ~\\src'"
    );
    assert_eq!(
        quote_argument("a'b\u{2018}c\u{2019}d\u{201A}e\u{201B}f", WIN),
        "'a''b\u{2018}\u{2018}c\u{2019}\u{2019}d\u{201A}\u{201A}e\u{201B}\u{201B}f'"
    );
    // A POSIX shell has only the ASCII quote.
    assert_eq!(quote_argument("Don\u{2019}t", POSIX), "'Don\u{2019}t'");
}
