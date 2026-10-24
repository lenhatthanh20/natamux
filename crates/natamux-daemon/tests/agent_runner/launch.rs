//! Groups of `agent-runner.test.ts`: the launch line, the prompt, the PATH and the mode.

use super::*;

#[test]
fn quotes_for_each_platform() {
    let cases = [
        ("plain", Platform::Posix, "'plain'"),
        ("it's", Platform::Posix, "'it'\\''s'"),
        ("$HOME `x` \"y\"", Platform::Posix, "'$HOME `x` \"y\"'"),
        ("it's", Platform::Windows, "'it''s'"),
    ];
    for (value, platform, quoted) in cases {
        assert_eq!(quote_argument(value, platform), quoted);
    }
}

#[test]
fn joins_the_title_and_the_description_on_one_line() {
    let env = Env::new();
    let task = env.task();
    let with = |description: &str| Task {
        title: "Fix it".into(),
        description: description.into(),
        ..task.clone()
    };
    assert_eq!(
        first_prompt(&with("Line one.\n\nLine  two.")),
        "Fix it - Line one. Line two."
    );
    assert_eq!(first_prompt(&with("  ")), "Fix it");
}

#[test]
fn runs_a_launch_script_with_the_hook_settings_and_the_first_prompt() {
    let env = Env::new();
    let task = env.task_with_workspace("Fix the bug", "Don't break it.");

    let updated = env
        .runner
        .start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    assert_eq!(env.pty_count(), 1);
    assert_eq!(
        env.pty(0).state().options.cwd,
        PathBuf::from(&env.workspace)
    );
    assert_eq!(env.pty(0).written(), vec![source_line(&env, &task)]);
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' --permission-mode auto 'Fix the bug - Don'\\''t break it.'\n"
    );
    assert_eq!(updated.agent.as_deref(), Some("claude-code"));
    let event = last_event(&env.tasks, &task.id);
    assert_eq!(event.kind, "agent.launched");
    assert_eq!(payload(&event, "agent"), json!("claude-code"));
}

#[test]
fn does_not_show_the_settings_path_or_the_prompt_in_the_terminal() {
    let env = Env::new();
    let task = env.task_with_workspace("Fix the bug", "A long secret description.");
    env.runner
        .start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    // The fake terminal echoes what is typed, like a real one. This is what a browser sees.
    let recorder = Arc::new(Recorder::default());
    let client: ClientRef = recorder.clone();
    let wide = Size {
        cols: 200,
        rows: 40,
    };
    let _detach = env
        .terminals
        .attach(&task.id, client, wide, "1", AttachOptions::default())
        .unwrap();
    while env.pty(0).deliver() {}
    env.terminals.settled(&task.id, "1");

    let text = recorder.output();
    assert!(text.contains("agent-launch-T-0001.sh"), "{text}");
    assert!(!text.contains("claude code.json"));
    assert!(!text.contains("--settings"));
    assert!(!text.contains("secret"));
}

#[test]
fn uses_the_prompt_it_is_given_and_the_shell_that_is_open() {
    let env = Env::new();
    let task = env.task();
    env.start(&task, "claude-code", "Only this");
    assert_eq!(env.script_text(&task.id), PROMPT_ONLY);
    env.start(&task, "claude-code", "  ");

    assert_eq!(env.pty_count(), 1);
    let line = source_line(&env, &task);
    assert_eq!(env.pty(0).written(), vec![line.clone(), line]);
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' --permission-mode auto\n"
    );
}

#[test]
fn types_the_command_itself_on_windows_where_scripts_may_be_refused() {
    let env = Env::new();
    let (windows, ptys) = env.windows("C:\\data\\settings.json", None);
    let task = env.task();

    windows
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    let written = ptys.lock().unwrap()[0].written();
    assert_eq!(
        written,
        vec![windows_line(
            "claude --settings 'C:\\data\\settings.json' --permission-mode auto 'Only this'"
        )]
    );
}

// PSReadLine takes these bytes as keys: ESC wipes the typed line and Ctrl+C cancels it, so
// an issue body could run its own command.
#[test]
fn types_no_control_bytes_of_the_prompt_on_windows() {
    let env = Env::new();
    let (windows, ptys) = env.windows("C:\\data\\settings.json", None);
    let task = env.task();

    let prompt = "a\x1bcalc.exe #\x03b";
    windows
        .start(
            &task.id,
            "claude-code",
            Some(prompt),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    let written = ptys.lock().unwrap()[0].written();
    assert_eq!(
        written,
        vec![windows_line(
            "claude --settings 'C:\\data\\settings.json' --permission-mode auto 'acalc.exe #b'"
        )]
    );
}

#[test]
fn takes_escape_sequences_and_control_bytes_out_of_the_first_prompt() {
    let env = Env::new();
    let task = env.task_with_workspace("Fix\x1b[2K the bug", "Do\x03 it\x1b]0;title\x07.");

    env.runner
        .start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' --permission-mode auto 'Fix the bug - Do it.'\n"
    );
}

#[test]
fn puts_the_natamux_command_first_on_the_path_of_the_agent() {
    let env = Env::new();
    let with_bin = AgentRunner::new(AgentRunnerOptions {
        bin_dir: Some("/Apps/Nata'mux/bin".into()),
        ..env.options()
    });
    let task = env.task();

    with_bin
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    assert_eq!(env.pty(0).written(), vec![source_line(&env, &task)]);
    assert_eq!(
        env.script_text(&task.id),
        format!("PATH='/Apps/Nata'\\''mux/bin':\"$PATH\" {PROMPT_ONLY}")
    );
}

#[test]
fn keeps_a_bin_folder_with_a_space_as_one_path_entry() {
    let env = Env::new();
    let with_bin = AgentRunner::new(AgentRunnerOptions {
        bin_dir: Some("/Users/me/Library/Application Support/Natamux/bin".into()),
        ..env.options()
    });
    let task = env.task();

    with_bin
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    assert!(
        env.script_text(&task.id).starts_with(
            "PATH='/Users/me/Library/Application Support/Natamux/bin':\"$PATH\" claude "
        )
    );
}

// The script is sourced by a real shell here. A fake natamux elsewhere on the PATH must lose.
#[cfg(unix)]
#[test]
fn runs_the_app_natamux_before_another_one_in_a_real_shell() {
    use std::os::unix::fs::PermissionsExt;
    let env = Env::new();
    let app_bin = env.data_dir.join("Application Support/bin");
    let other_bin = env.data_dir.join("other");
    for (dir, name) in [(&app_bin, "app"), (&other_bin, "other")] {
        std::fs::create_dir_all(dir).unwrap();
        let file = dir.join("natamux");
        std::fs::write(&file, format!("#!/bin/sh\necho {name}\n")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let task = env.task();
    let source = |bin_dir: Option<&Path>| {
        AgentRunner::new(AgentRunnerOptions {
            claude_settings_path: "/data/settings.json".into(),
            bin_dir: bin_dir.map(|dir| dir.to_string_lossy().into_owned()),
            programs: HashMap::from([("claude-code".to_string(), "natamux".to_string())]),
            ..env.options()
        })
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", ". \"$1\"", "sh"])
            .arg(env.script(&task.id))
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", other_bin.display()))
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    assert_eq!(source(Some(&app_bin)), "app");
    // Without a bin folder (a daemon not started by the app) the script still runs.
    assert_eq!(source(None), "other");
}

#[test]
fn leaves_the_path_alone_on_windows_even_with_a_bin_folder() {
    let env = Env::new();
    let (windows, ptys) = env.windows("C:\\data\\settings.json", Some("C:\\Natamux\\bin"));
    let task = env.task();

    windows
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    let written = ptys.lock().unwrap()[0].written();
    assert_eq!(
        written,
        vec![windows_line(
            "claude --settings 'C:\\data\\settings.json' --permission-mode auto 'Only this'"
        )]
    );
}

#[test]
fn reads_the_permission_mode_of_the_project_at_each_start() {
    let env = Env::new();
    let mode = Arc::new(Mutex::new("ask".to_string()));
    let read = Arc::clone(&mode);
    let asking = AgentRunner::new(AgentRunnerOptions {
        permission_mode_of: Arc::new(move |_| read.lock().unwrap().clone()),
        ..env.options()
    });
    let task = env.task();

    asking
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' 'Only this'\n"
    );
    *mode.lock().unwrap() = "auto".into();
    asking
        .start(
            &task.id,
            "claude-code",
            Some("Only this"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
    assert_eq!(env.script_text(&task.id), PROMPT_ONLY);
}
