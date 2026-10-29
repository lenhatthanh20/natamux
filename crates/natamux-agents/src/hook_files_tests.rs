use super::*;
use crate::test_support::{TempDir, git_repo, git_status};
use natamux_core::agents::hook_settings::claude_code_hook_settings;
use natamux_proto::agents::claude_code::CLAUDE_CODE_HOOK_EVENTS;
use serde_json::json;

fn read(repo: &Path, path: &str) -> String {
    fs::read_to_string(repo.join(path)).unwrap()
}

fn write(repo: &Path, path: &str, text: &str) {
    let target = repo.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, text).unwrap();
}

fn none() -> WorkspaceHooks {
    WorkspaceHooks::default()
}

fn claude_on() -> WorkspaceHooks {
    WorkspaceHooks { claude_code: true }
}

#[test]
fn writes_the_hook_file_and_keeps_it_out_of_git_once() {
    let Some(repo) = git_repo("launch") else {
        return;
    };
    let path = repo.path();
    assert!(prepare_workspace("copilot", path, none()));
    assert!(prepare_workspace("copilot", path, none()));
    assert!(path.join(".github/hooks/natamux.json").exists());
    let exclude = read(path, ".git/info/exclude");
    let lines = exclude
        .split('\n')
        .filter(|line| *line == "/.github/hooks/natamux.json")
        .count();
    assert_eq!(lines, 1);
    assert_eq!(git_status(path), "");
}

#[test]
fn leaves_the_users_own_cursor_hooks_alone() {
    let repo = TempDir::new("launch");
    write(
        repo.path(),
        ".cursor/hooks.json",
        r#"{"version":1,"hooks":{"stop":[{"command":"say done"}]}}"#,
    );
    assert!(!prepare_workspace("cursor", repo.path(), none()));
    assert!(read(repo.path(), ".cursor/hooks.json").contains("say done"));
}

#[test]
fn keeps_a_cursor_file_that_mixes_natamuxs_hooks_with_one_of_the_users_own() {
    let repo = TempDir::new("launch");
    let text = r#"{"version":1,"hooks":{"stop":[{"command":"natamux hook cursor"},{"command":"say done"}]}}"#;
    write(repo.path(), ".cursor/hooks.json", text);
    assert!(!prepare_workspace("cursor", repo.path(), none()));
    prepare_agent_hooks(repo.path(), none());
    assert_eq!(read(repo.path(), ".cursor/hooks.json"), text);
}

#[test]
fn updates_a_file_with_only_natamux_hooks_such_as_one_from_an_older_version() {
    let repo = TempDir::new("launch");
    write(
        repo.path(),
        ".cursor/hooks.json",
        r#"{"version":1,"hooks":{"stop":[{"command":"natamux hook cursor"}]}}"#,
    );
    assert!(prepare_workspace("cursor", repo.path(), none()));
    assert_eq!(
        read(repo.path(), ".cursor/hooks.json"),
        hook_file_text_of(&cursor_hook_file())
    );
}

#[test]
fn keeps_a_copilot_file_with_a_bash_or_powershell_hook_of_the_users_own() {
    let repo = TempDir::new("launch");
    let command = "natamux hook copilot agentStop";
    for own in ["bash", "powershell"] {
        let mut hook = json!({ "bash": command, "powershell": command });
        hook[own] = json!("say done");
        let text = json!({ "version": 1, "hooks": { "agentStop": [hook] } }).to_string();
        write(repo.path(), ".github/hooks/natamux.json", &text);
        assert!(!prepare_workspace("copilot", repo.path(), none()));
        assert_eq!(read(repo.path(), ".github/hooks/natamux.json"), text);
    }
}

#[test]
fn updates_a_copilot_file_with_only_natamux_hooks() {
    let repo = TempDir::new("launch");
    let command = "natamux hook copilot agentStop";
    let hooks = json!({ "agentStop": [{ "bash": command, "powershell": command }] });
    write(
        repo.path(),
        ".github/hooks/natamux.json",
        &json!({ "version": 1, "hooks": hooks }).to_string(),
    );
    assert!(prepare_workspace("copilot", repo.path(), none()));
    assert_eq!(
        read(repo.path(), ".github/hooks/natamux.json"),
        hook_file_text_of(&copilot_hook_file())
    );
}

#[test]
fn keeps_a_file_that_is_not_json_has_no_hook_command_or_has_one_not_from_natamux() {
    let own = r#"{"command":"natamux hook cursor"}"#;
    let texts = [
        String::new(),
        " \n".into(),
        "natamux hook cursor {".into(),
        r#"{"version":1,"hooks":{}}"#.into(),
        r#"{"version":1}"#.into(),
        format!(r#"{{"hooks":[{own}]}}"#),
        r#"{"hooks":"natamux hook cursor"}"#.into(),
        r#"{"hooks":null}"#.into(),
        r#"{"hooks":{"stop":[{"timeout":5}]}}"#.into(),
        format!(r#"{{"hooks":{{"stop":[{own},{{"command":5}}]}}}}"#),
        format!(r#"{{"hooks":{{"stop":[{own},{{"command":null}}]}}}}"#),
        format!("[{own}]"),
        "null".into(),
        "5".into(),
        r#"{"hooks":{"stop":[{"command":"say natamux hook"}]}}"#.into(),
        r#"{"hooks":{"stop":[{"command":"echo natamux hook cursor"}]}}"#.into(),
        // A command must start with `natamux hook`, so leading space makes it the user's own.
        r#"{"hooks":{"stop":[{"command":" natamux hook cursor"}]}}"#.into(),
        format!(
            r#"{{"hooks":{{"stop":[{own}],"afterFileEdit":[{own},{{"command":"say done"}}]}}}}"#
        ),
    ];
    let repo = TempDir::new("launch");
    for text in texts {
        write(repo.path(), ".cursor/hooks.json", &text);
        assert!(!prepare_workspace("cursor", repo.path(), none()), "{text}");
        assert_eq!(read(repo.path(), ".cursor/hooks.json"), text);
    }
}

#[test]
fn keeps_a_copilot_file_with_a_foreign_bash_hook_alone_or_in_a_later_event() {
    let command = "natamux hook copilot agentStop";
    let own = json!({ "bash": command, "powershell": command });
    let foreign = json!({ "bash": "say done", "powershell": command });
    let texts = [
        json!({ "agentStop": [{ "bash": "say done" }] }),
        json!({ "agentStop": [own.clone()], "errorOccurred": [own, foreign] }),
    ]
    .map(|hooks| json!({ "version": 1, "hooks": hooks }).to_string());
    let repo = TempDir::new("launch");
    for text in texts {
        write(repo.path(), ".github/hooks/natamux.json", &text);
        assert!(!prepare_workspace("copilot", repo.path(), none()), "{text}");
        assert_eq!(read(repo.path(), ".github/hooks/natamux.json"), text);
    }
}

#[test]
fn writes_the_hook_files_of_all_agents_for_an_agent_started_by_hand_in_a_shell() {
    let Some(repo) = git_repo("launch") else {
        return;
    };
    prepare_agent_hooks(repo.path(), none());
    assert!(read(repo.path(), ".cursor/hooks.json").contains("natamux hook cursor"));
    assert!(repo.path().join(".github/hooks/natamux.json").exists());
    assert_eq!(git_status(repo.path()), "");
}

#[test]
fn writes_no_claude_code_file_unless_the_daemon_asks_for_it() {
    let repo = TempDir::new("launch");
    assert!(!prepare_workspace("claude-code", repo.path(), none()));
    assert!(!repo.path().join(".claude").exists());
}

#[test]
fn writes_each_one_with_the_hooks_of_the_launch_out_of_git() {
    let Some(repo) = git_repo("launch") else {
        return;
    };
    let path = repo.path();
    prepare_agent_hooks(path, claude_on());

    assert_eq!(
        read(path, ".claude/settings.local.json"),
        hook_file_text_of(&claude_code_workspace_settings(
            std::env::var("NATAMUX_BIN_DIR").ok().as_deref()
        ))
    );
    assert_eq!(
        read(path, ".gemini/settings.json"),
        hook_file_text_of(&gemini_hook_settings())
    );
    let codex = read(path, ".codex/config.toml");
    assert_eq!(codex, codex_hook_file());
    // The same values as the `-c` overrides of a launch, so those replace them one by one.
    let args: Vec<String> = crate::launch::codex_hook_args()
        .into_iter()
        .filter(|arg| arg != "-c")
        .map(|arg| {
            let (key, value) = arg.split_once('=').unwrap();
            format!("{} = {value}", key.trim_start_matches("hooks."))
        })
        .collect();
    let lines: Vec<&str> = codex
        .split('\n')
        .filter(|line| line.contains('=') && !line.starts_with('#'))
        .collect();
    assert_eq!(lines, args);
    assert_eq!(git_status(path), "");
}

#[test]
fn leaves_the_users_own_files_alone() {
    let repo = TempDir::new("launch");
    let path = repo.path();
    let own = [
        (
            ".claude/settings.local.json",
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#,
        ),
        (
            ".gemini/settings.json",
            r#"{"model":{"name":"gemini-pro"}}"#,
        ),
        (".codex/config.toml", "model = \"o3\"\n"),
    ];
    for (file, text) in own {
        write(path, file, text);
    }
    assert!(!prepare_workspace("claude-code", path, claude_on()));
    assert!(!prepare_workspace("gemini", path, none()));
    assert!(!prepare_workspace("codex", path, none()));
    for (file, text) in own {
        assert_eq!(read(path, file), text);
    }
}

#[test]
fn gives_claude_code_commands_not_http_hooks_since_copilot_cli_reads_its_file_too() {
    let hook = json!({ "type": "command", "command": "natamux hook claude-code", "timeout": 5 });
    let hooks: serde_json::Map<String, Value> = CLAUDE_CODE_HOOK_EVENTS
        .iter()
        .map(|event| ((*event).to_string(), json!([{ "hooks": [hook.clone()] }])))
        .collect();
    assert_eq!(claude_code_hook_file(), json!({ "hooks": hooks }));
}

#[test]
fn writes_the_hooks_of_an_older_natamux_again_and_keeps_what_claude_code_saved() {
    let repo = TempDir::new("launch");
    let mut old = claude_code_hook_settings("http://127.0.0.1:1111");
    old["permissions"] = json!({ "allow": ["Bash(pnpm test)"] });
    write(
        repo.path(),
        ".claude/settings.local.json",
        &hook_file_text_of(&old),
    );

    assert!(prepare_workspace("claude-code", repo.path(), claude_on()));

    let written: Value =
        serde_json::from_str(&read(repo.path(), ".claude/settings.local.json")).unwrap();
    let mut want = claude_code_workspace_settings(std::env::var("NATAMUX_BIN_DIR").ok().as_deref());
    want["permissions"] = json!({ "allow": ["Bash(pnpm test)"] });
    assert_eq!(written, want);
}

#[test]
fn keeps_a_status_line_of_the_users_own_in_the_claude_code_file() {
    let repo = TempDir::new("launch");
    let mut old = claude_code_hook_file();
    old["statusLine"] = json!({ "type": "command", "command": "~/.claude/line.sh" });
    write(
        repo.path(),
        ".claude/settings.local.json",
        &hook_file_text_of(&old),
    );

    assert!(prepare_workspace("claude-code", repo.path(), claude_on()));

    let written: Value =
        serde_json::from_str(&read(repo.path(), ".claude/settings.local.json")).unwrap();
    assert_eq!(written["statusLine"]["command"], "~/.claude/line.sh");
}

#[test]
fn the_status_line_runs_the_app_s_own_natamux_when_the_daemon_knows_it() {
    // Built with Path::join, as the code does: Windows uses `\`.
    let bin = Path::new("/Applications/Natamux.app/Contents/MacOS").join("natamux");
    assert_eq!(
        claude_code_status_line(Some("/Applications/Natamux.app/Contents/MacOS"))["command"],
        format!("\"{}\" statusline", bin.display())
    );
    assert_eq!(
        claude_code_status_line(None)["command"],
        "natamux statusline"
    );
    assert!(is_natamux_status_line(&claude_code_status_line(Some(
        "/a b"
    ))));
    assert!(!is_natamux_status_line(
        &json!({ "command": "~/.claude/line.sh" })
    ));
}

#[test]
fn writes_a_codex_file_of_an_older_natamux_again_by_its_first_line() {
    let repo = TempDir::new("launch");
    let first = codex_hook_file().split('\n').next().unwrap().to_string();
    write(
        repo.path(),
        ".codex/config.toml",
        &format!("{first}\n[hooks]\n"),
    );

    assert!(prepare_workspace("codex", repo.path(), none()));
    assert_eq!(read(repo.path(), ".codex/config.toml"), codex_hook_file());
}

#[test]
fn does_not_take_an_http_hook_without_the_task_header_for_one_of_natamux() {
    let repo = TempDir::new("launch");
    let text =
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"http","url":"http://x/hooks/claude-code"}]}]}}"#;
    write(repo.path(), ".claude/settings.local.json", text);

    assert!(!prepare_workspace("claude-code", repo.path(), claude_on()));
    assert_eq!(read(repo.path(), ".claude/settings.local.json"), text);
}

// Not in Node's tests: the ignore list is written the same without a git program.
#[test]
fn adds_a_line_to_the_exclude_file_once_with_a_line_break_before_it_when_needed() {
    let dir = TempDir::new("launch");
    let common = Path::new(".git");
    write(dir.path(), ".git/info/exclude", "# mine");
    add_exclude_line(dir.path(), common, "/a").unwrap();
    add_exclude_line(dir.path(), common, "/a").unwrap();
    add_exclude_line(dir.path(), &dir.path().join(".git"), "/b").unwrap();
    assert_eq!(read(dir.path(), ".git/info/exclude"), "# mine\n/a\n/b\n");
    let missing = TempDir::new("launch");
    add_exclude_line(missing.path(), common, "/a").unwrap();
    assert_eq!(read(missing.path(), ".git/info/exclude"), "/a\n");
}

#[cfg(unix)]
#[test]
fn writes_no_hook_file_through_a_link_in_the_worktree() {
    use std::os::unix::fs::symlink;
    let targets = [
        ("claude-code", ".claude/settings.local.json"),
        ("gemini", ".gemini/settings.json"),
        ("codex", ".codex/config.toml"),
        ("copilot", ".github/hooks/natamux.json"),
        ("cursor", ".cursor/hooks.json"),
    ];
    let outside = TempDir::new("outside");
    for (agent, file) in targets {
        let top = file.split('/').next().unwrap();
        // A link as the top folder.
        let folder = TempDir::new("launch");
        symlink(outside.path(), folder.path().join(top)).unwrap();
        assert!(
            !prepare_workspace(agent, folder.path(), claude_on()),
            "{agent}"
        );
        // A link as the file, to a file that is there and to nothing.
        for goal in ["secret", "missing"] {
            let repo = TempDir::new("launch");
            let target = repo.path().join(file);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(outside.path().join("secret"), "keep").unwrap();
            symlink(outside.path().join(goal), &target).unwrap();
            assert!(
                !prepare_workspace(agent, repo.path(), claude_on()),
                "{agent}"
            );
        }
    }
    assert_eq!(read(outside.path(), "secret"), "keep");
    assert!(!outside.path().join("missing").exists());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 1);
}
