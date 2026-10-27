use super::*;

const POSIX: Platform = Platform::Posix;
const WIN: Platform = Platform::Windows;

fn base() -> LaunchOptions {
    LaunchOptions {
        program: "x".into(),
        extra_args: String::new(),
        prompt: "Fix it".into(),
        session: None,
        platform: POSIX,
        claude_settings_path: "/data/claude.json".into(),
        gemini_settings_path: "/data/gemini.json".into(),
        auto: false,
        workspace: None,
        workspace_hooks: false,
    }
}

fn auto() -> LaunchOptions {
    LaunchOptions {
        auto: true,
        ..base()
    }
}

fn with(options: LaunchOptions, program: &str) -> LaunchOptions {
    LaunchOptions {
        program: program.into(),
        ..options
    }
}

fn launch(agent: &str, options: LaunchOptions) -> String {
    launch_command(agent, &options)
}

const GEMINI_ENV: &str =
    "GEMINI_CLI_TRUST_WORKSPACE=true GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'";
const CODEX_STOP: &str =
    r#"'hooks.Stop=[{hooks=[{type="command",command="natamux hook codex",timeout=5}]}]'"#;

#[test]
fn starts_each_agent_in_its_auto_mode_before_the_extra_words_and_the_prompt() {
    let with_x = |program: &str| LaunchOptions {
        extra_args: "--x".into(),
        ..with(auto(), program)
    };
    assert_eq!(
        launch("claude-code", with_x("claude")),
        "claude --settings '/data/claude.json' --permission-mode auto --x 'Fix it'"
    );
    assert_eq!(
        launch("copilot", with_x("copilot")),
        "copilot --allow-all-tools --x -i 'Fix it'"
    );
    assert_eq!(
        launch("cursor", with_x("cursor-agent")),
        "cursor-agent --force --x 'Fix it'"
    );
    assert_eq!(
        launch("gemini", with_x("gemini")),
        format!("{GEMINI_ENV} gemini --approval-mode yolo --x -i 'Fix it'")
    );
    let codex = launch("codex", with_x("codex"));
    assert!(codex.starts_with(
        "codex --dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never -c "
    ));
    assert!(!codex.contains("--full-auto"));
    assert!(codex.ends_with(" --x 'Fix it'"));
}

#[test]
fn adds_the_auto_mode_on_windows_and_without_a_prompt() {
    let no_prompt = LaunchOptions {
        prompt: String::new(),
        ..with(auto(), "claude")
    };
    assert_eq!(
        launch("claude-code", no_prompt),
        "claude --settings '/data/claude.json' --permission-mode auto"
    );
    let quoted = LaunchOptions {
        platform: WIN,
        prompt: "it's".into(),
        ..with(auto(), "claude")
    };
    assert_eq!(
        launch("claude-code", quoted),
        "claude --settings '/data/claude.json' --permission-mode auto 'it''s'"
    );
    let gemini = LaunchOptions {
        platform: WIN,
        prompt: String::new(),
        ..with(auto(), "gemini")
    };
    assert_eq!(
        launch("gemini", gemini),
        "$env:GEMINI_CLI_TRUST_WORKSPACE='true'; $env:GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'; gemini --approval-mode yolo"
    );
}

#[test]
fn starts_copilot_without_an_alt_screen_flag_in_every_mode() {
    let plain = LaunchOptions {
        prompt: String::new(),
        ..with(base(), "copilot")
    };
    assert_eq!(launch("copilot", plain), "copilot");
    let windows = LaunchOptions {
        platform: WIN,
        prompt: "it's".into(),
        ..with(auto(), "copilot")
    };
    assert_eq!(
        launch("copilot", windows),
        "copilot --allow-all-tools -i 'it''s'"
    );
}

#[test]
fn starts_codex_inline_in_every_mode_before_the_extra_words() {
    for options in [base(), auto()] {
        let options = LaunchOptions {
            extra_args: "--x".into(),
            ..with(options, "codex")
        };
        let codex = launch("codex", options);
        assert_eq!(codex.split(" --no-alt-screen ").count(), 2);
        assert!(codex.ends_with(" --no-alt-screen --x 'Fix it'"));
    }
}

#[test]
fn adds_no_second_no_alt_screen_when_the_extra_words_hold_it() {
    let extra = "--x  --no-alt-screen";
    let options = LaunchOptions {
        extra_args: extra.into(),
        ..with(auto(), "codex")
    };
    let codex = launch("codex", options);
    assert_eq!(codex.split("--no-alt-screen").count(), 2);
    assert!(codex.ends_with(&format!(" {extra} 'Fix it'")));
}

#[test]
fn goes_on_with_a_claude_code_session_after_the_mode() {
    let options = LaunchOptions {
        prompt: String::new(),
        session: Some("s-1".into()),
        ..with(auto(), "claude")
    };
    assert_eq!(
        launch("claude-code", options),
        "claude --settings '/data/claude.json' --permission-mode auto --resume 's-1'"
    );
}

#[test]
fn goes_on_with_the_newest_claude_code_session_of_the_folder() {
    let options = LaunchOptions {
        prompt: String::new(),
        session: Some(LAST_SESSION.into()),
        ..base()
    };
    assert_eq!(
        launch("claude-code", options),
        "x --settings '/data/claude.json' --continue"
    );
}

#[test]
fn quotes_the_session_so_it_cannot_run_as_a_command() {
    let session = "s'; rm -rf ~; '";
    let options = LaunchOptions {
        prompt: String::new(),
        session: Some(session.into()),
        ..with(base(), "claude")
    };
    assert_eq!(
        launch("claude-code", options),
        format!(
            "claude --settings '/data/claude.json' --resume {}",
            quote_argument(session, POSIX)
        )
    );
    assert_eq!(quote_argument(session, POSIX), r"'s'\''; rm -rf ~; '\'''");
}

#[test]
fn goes_on_with_a_session_of_each_other_agent_with_its_mode_and_extra_words() {
    let resume = |program: &str| LaunchOptions {
        extra_args: "--x".into(),
        prompt: String::new(),
        session: Some("s-1".into()),
        ..with(auto(), program)
    };
    assert_eq!(
        launch("copilot", resume("copilot")),
        "copilot --allow-all-tools --x --resume 's-1'"
    );
    assert_eq!(
        launch("cursor", resume("cursor-agent")),
        "cursor-agent --force --x --resume 's-1'"
    );
    assert_eq!(
        launch("gemini", resume("gemini")),
        format!("{GEMINI_ENV} gemini --approval-mode yolo --x --resume 's-1'")
    );
    let codex = launch("codex", resume("codex"));
    assert!(codex.starts_with(
        "codex resume --dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never -c "
    ));
    assert!(codex.contains(CODEX_STOP));
    assert!(codex.ends_with(" --no-alt-screen --x 's-1'"));
}

#[test]
fn goes_on_with_the_newest_session_of_the_folder_for_each_agent() {
    let newest = |program: &str| LaunchOptions {
        prompt: String::new(),
        session: Some(LAST_SESSION.into()),
        ..with(base(), program)
    };
    assert_eq!(launch("copilot", newest("copilot")), "copilot --continue");
    assert_eq!(
        launch("cursor", newest("cursor-agent")),
        "cursor-agent --continue"
    );
    assert!(launch("gemini", newest("gemini")).ends_with(" gemini --resume"));
    let codex = launch("codex", newest("codex"));
    assert!(codex.starts_with("codex resume "));
    assert!(codex.ends_with(" --no-alt-screen --last"));
}

#[test]
fn quotes_the_session_of_every_agent_so_it_cannot_run_as_a_command() {
    let session = "s'; rm -rf ~; '";
    for agent in LAUNCHABLE_AGENTS {
        let options = LaunchOptions {
            prompt: String::new(),
            session: Some(session.into()),
            ..base()
        };
        let command = launch(agent, options);
        assert!(command.ends_with(&format!(" {}", quote_argument(session, POSIX))));
    }
}

// A paused agent woken by a stage or a review comment goes on with its session and that prompt.
#[test]
fn goes_on_with_the_session_and_the_prompt_for_every_agent() {
    for agent in LAUNCHABLE_AGENTS {
        let options = LaunchOptions {
            prompt: "Review it".into(),
            session: Some("s-1".into()),
            ..base()
        };
        let command = launch(agent, options);
        let session = command.find(" 's-1'");
        let prompt = command.find(" 'Review it'");
        assert!(
            session.is_some() && prompt.is_some() && session < prompt,
            "{agent}: {command}"
        );
        assert!(command.ends_with(" 'Review it'"), "{agent}: {command}");
    }
}

#[test]
fn quotes_the_prompt_of_each_agent_for_powershell_next_to_its_flag() {
    for (agent, flag) in [
        ("claude-code", "--permission-mode auto"),
        (
            "codex",
            "--sandbox workspace-write --ask-for-approval never",
        ),
        ("gemini", "--approval-mode yolo"),
        ("copilot", "--allow-all-tools"),
        ("cursor", "--force"),
    ] {
        let options = LaunchOptions {
            platform: WIN,
            prompt: "it's".into(),
            ..auto()
        };
        let command = launch(agent, options);
        assert!(command.contains(&format!(" {flag} ")), "{agent}");
        assert!(command.ends_with(" 'it''s'"), "{agent}");
    }
}

#[test]
fn starts_each_agent_interactive_with_its_first_prompt_and_its_hooks() {
    assert_eq!(
        launch("claude-code", with(base(), "claude")),
        "claude --settings '/data/claude.json' 'Fix it'"
    );
    let copilot = LaunchOptions {
        extra_args: "--allow-all-tools".into(),
        ..with(base(), "copilot")
    };
    assert_eq!(
        launch("copilot", copilot),
        "copilot --allow-all-tools -i 'Fix it'"
    );
    assert_eq!(
        launch("cursor", with(base(), "cursor-agent")),
        "cursor-agent 'Fix it'"
    );
    assert_eq!(
        launch("gemini", with(base(), "gemini")),
        format!("{GEMINI_ENV} gemini -i 'Fix it'")
    );
    let codex = launch("codex", with(base(), "codex"));
    assert!(codex.starts_with("codex --dangerously-bypass-hook-trust -c "));
    assert!(!codex.contains("--sandbox"));
    assert!(!codex.contains("--ask-for-approval"));
    assert!(codex.contains(CODEX_STOP));
    assert!(codex.ends_with(" 'Fix it'"));
}

#[test]
fn adds_the_codex_sandbox_flags_once_each_in_order_and_keeps_its_hooks() {
    let prompt = "Don't break 'Application Support' or \"$HOME\"";
    let asking = launch(
        "codex",
        LaunchOptions {
            prompt: prompt.into(),
            ..with(base(), "codex")
        },
    );
    let codex = launch(
        "codex",
        LaunchOptions {
            prompt: prompt.into(),
            ..with(auto(), "codex")
        },
    );
    assert_eq!(
        codex,
        asking.replacen(
            "--dangerously-bypass-hook-trust ",
            "--dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never ",
            1
        )
    );
    assert_eq!(codex.split(" --sandbox ").count(), 2);
    assert_eq!(codex.split(" --ask-for-approval ").count(), 2);
    assert!(codex.ends_with(&format!(" {}", quote_argument(prompt, POSIX))));
}

#[test]
fn adds_no_auto_flag_of_any_agent_to_each_agent_in_the_ask_mode() {
    for agent in LAUNCHABLE_AGENTS {
        let command = launch(agent, with(base(), agent));
        for (_, flags) in AUTO_FLAGS {
            assert!(!command.contains(&flags.join(" ")), "{agent}");
        }
        assert!(!command.contains("--full-auto"));
    }
}

#[test]
fn keeps_the_codex_hooks_in_the_ask_mode() {
    let codex = launch("codex", with(base(), "codex"));
    assert!(codex.contains(" --dangerously-bypass-hook-trust "));
    assert_eq!(codex.split(" -c ").count(), codex_hook_args().len() / 2 + 1);
}

#[test]
fn adds_no_flags_for_a_mode_other_than_auto() {
    // Any other mode is `auto: false`; the daemon maps its setting to this flag.
    let other = LaunchOptions {
        auto: false,
        ..base()
    };
    assert_eq!(
        launch("codex", with(other, "codex")),
        launch("codex", with(base(), "codex"))
    );
}

#[test]
fn puts_the_words_of_natamux_codex_args_after_the_codex_mode_flags() {
    let extra = r#"--sandbox read-only --ask-for-approval on-request --model "o3 mini""#;
    let codex = launch(
        "codex",
        LaunchOptions {
            extra_args: extra.into(),
            ..with(auto(), "codex")
        },
    );
    let at = codex.find(extra).unwrap();
    assert!(
        at > codex
            .find("--sandbox workspace-write --ask-for-approval never")
            .unwrap()
    );
    assert!(at > codex.rfind(" -c ").unwrap());
    assert!(codex.ends_with(&format!(" {extra} 'Fix it'")));
}

#[test]
fn adds_the_words_of_natamux_codex_args_without_mode_flags_in_the_ask_mode() {
    let codex = launch(
        "codex",
        LaunchOptions {
            extra_args: "--model o3".into(),
            ..with(base(), "codex")
        },
    );
    assert!(codex.ends_with(" --model o3 'Fix it'"));
    assert!(!codex.contains("--sandbox"));
}

#[test]
fn sets_the_gemini_settings_in_powershell_on_windows_and_leaves_the_prompt_out_when_empty() {
    let options = LaunchOptions {
        platform: WIN,
        prompt: String::new(),
        ..with(base(), "gemini")
    };
    assert_eq!(
        launch("gemini", options),
        "$env:GEMINI_CLI_TRUST_WORKSPACE='true'; $env:GEMINI_CLI_SYSTEM_SETTINGS_PATH='/data/gemini.json'; gemini"
    );
}

#[test]
fn trusts_the_workspace_for_this_codex_run_before_its_hooks() {
    let workspace = "/Users/me/Library/Application Support/Natamux/data/worktrees/T-0001";
    let codex = launch(
        "codex",
        LaunchOptions {
            workspace: Some(workspace.into()),
            ..with(auto(), "codex")
        },
    );
    assert!(codex.contains(&format!(
        "--ask-for-approval never -c 'projects={{\"{workspace}\"={{trust_level=\"trusted\"}}}}' -c 'hooks."
    )));
    assert!(!launch("codex", with(base(), "codex")).contains("projects="));
}

#[test]
fn escapes_a_windows_path_in_the_codex_trust_override() {
    let codex = launch(
        "codex",
        LaunchOptions {
            platform: WIN,
            workspace: Some("C:\\work\\T-0001".into()),
            ..with(base(), "codex")
        },
    );
    assert!(codex.contains(r#"-c 'projects={"C:\\work\\T-0001"={trust_level="trusted"}}'"#));
}

#[test]
fn keeps_a_quote_in_the_worktree_path_inside_one_shell_word_and_one_toml_string() {
    let codex = launch(
        "codex",
        LaunchOptions {
            workspace: Some(r#"/w/it's "x""#.into()),
            ..with(base(), "codex")
        },
    );
    assert!(
        codex.contains(r#" -c 'projects={"/w/it'\''s \"x\""={trust_level="trusted"}}' -c 'hooks."#)
    );
}

#[test]
fn leaves_out_the_claude_code_settings_when_the_workspace_has_the_hooks() {
    let hooks = LaunchOptions {
        workspace_hooks: true,
        ..base()
    };
    assert_eq!(launch("claude-code", hooks), "x 'Fix it'");
    assert_eq!(
        launch("claude-code", base()),
        "x --settings '/data/claude.json' 'Fix it'"
    );
}

#[test]
fn leaves_out_the_gemini_system_settings_when_the_workspace_has_the_hooks() {
    let gemini = LaunchOptions {
        prompt: String::new(),
        workspace_hooks: true,
        ..base()
    };
    assert_eq!(
        launch("gemini", gemini.clone()),
        "GEMINI_CLI_TRUST_WORKSPACE=true x"
    );
    assert_eq!(
        launch(
            "gemini",
            LaunchOptions {
                platform: WIN,
                ..gemini
            }
        ),
        "$env:GEMINI_CLI_TRUST_WORKSPACE='true'; x"
    );
}

#[test]
fn keeps_codexs_hooks_on_the_launch_line() {
    // Codex's `-c` replaces a key of the workspace file.
    let own = LaunchOptions {
        workspace_hooks: true,
        ..base()
    };
    assert!(launch("codex", own).contains("hooks.Stop="));
}
