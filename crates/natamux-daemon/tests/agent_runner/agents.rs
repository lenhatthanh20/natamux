//! Groups of `agent-runner.test.ts`: trust, extra words, own agents, refusals and Copilot.

use super::*;

#[test]
fn starts_codex_in_its_sandbox_without_asking_when_the_project_sets_no_mode() {
    let env = Env::new();
    let task = env.task();
    env.start(&task, "codex", "Only this");
    assert!(env.script_text(&task.id).starts_with(
        "codex --dangerously-bypass-hook-trust --sandbox workspace-write --ask-for-approval never -c "
    ));
}

#[test]
fn trusts_the_task_workspace_for_codex_and_cursor() {
    let env = Env::new();
    let task = env.task();
    env.start(&task, "codex", "Only this");
    // The path goes in as a JSON string (Windows' backslashes doubled).
    assert!(env.script_text(&task.id).contains(&format!(
        " -c 'projects={{{}={{trust_level=\"trusted\"}}}}' ",
        serde_json::to_string(&env.workspace).unwrap()
    )));
    env.start(&task, "cursor", "Only this");
    let slug: String = env
        .workspace
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let marker = env
        .data_dir
        .join(".cursor/projects")
        .join(slug)
        .join(".workspace-trusted");
    let text: Value = serde_json::from_str(&std::fs::read_to_string(marker).unwrap()).unwrap();
    assert_eq!(text["workspacePath"], json!(env.workspace));
    env.start(&task, "gemini", "Only this");
    assert!(
        env.script_text(&task.id)
            .starts_with("GEMINI_CLI_TRUST_WORKSPACE=true ")
    );
}

#[test]
fn starts_the_agent_when_its_trust_cannot_be_written() {
    let env = Env::new();
    let file = env.data_dir.join("a-file");
    std::fs::write(&file, "").unwrap();
    let broken = AgentRunner::new(AgentRunnerOptions {
        trust_paths: trust_paths(|_| None, &file.join("home")),
        ..env.options()
    });
    let task = env.task();
    for agent in ["cursor", "copilot"] {
        broken
            .start(
                &task.id,
                agent,
                Some("Only this"),
                None,
                None,
                &OwnAgent::default(),
            )
            .unwrap();
        assert_eq!(
            env.tasks.get_task(&task.id).unwrap().agent.as_deref(),
            Some(agent)
        );
    }
    assert_eq!(
        env.script_text(&task.id),
        "copilot --allow-all-tools -i 'Only this'\n"
    );
}

#[test]
fn adds_no_extra_word_for_natamux_codex_args_of_blank_values() {
    for words in ["", " ", "  \t "] {
        let env = Env::new();
        let spaced = AgentRunner::new(AgentRunnerOptions {
            extra_args: HashMap::from([("codex".to_string(), words.to_string())]),
            ..env.options()
        });
        let task = env.task();
        spaced
            .start(
                &task.id,
                "codex",
                Some("Only this"),
                None,
                None,
                &OwnAgent::default(),
            )
            .unwrap();
        let script = env.script_text(&task.id);
        assert!(script.ends_with(" 'Only this'\n"), "{script:?}");
        assert!(!script.ends_with("  'Only this'\n"), "{script:?}");
    }
}

#[test]
fn starts_a_stages_own_agent_with_its_words_after_the_configured_ones_and_keeps_the_task_agent() {
    let env = Env::new();
    let configured = AgentRunner::new(AgentRunnerOptions {
        extra_args: HashMap::from([("codex".to_string(), "--foo".to_string())]),
        ..env.options()
    });
    let task = env.task();
    configured
        .start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    let own = OwnAgent {
        args: Some("--model gpt-5".into()),
        stage: Some("test-spec".into()),
        restored: false,
    };
    let updated = configured
        .start(&task.id, "codex", Some("Only this"), Some("2"), None, &own)
        .unwrap();

    let script =
        std::fs::read_to_string(env.data_dir.join(format!("agent-launch-{}-2.sh", task.id)));
    assert!(script.unwrap().contains("--foo --model gpt-5 'Only this'"));
    assert_eq!(updated.agent.as_deref(), Some("claude-code"));
    let event = last_event(&env.tasks, &task.id);
    assert_eq!(event.kind, "agent.launched");
    assert_eq!(payload(&event, "agent"), json!("codex"));
    assert_eq!(payload(&event, "stage"), json!("test-spec"));
}

#[test]
fn restores_an_agent_on_its_session_in_its_shell_and_keeps_the_task_agent() {
    let env = Env::new();
    let launches: Arc<Mutex<Vec<AgentLaunch>>> = Arc::default();
    let sink = Arc::clone(&launches);
    let restoring = AgentRunner::new(AgentRunnerOptions {
        on_launch: Some(Arc::new(move |launch: &AgentLaunch| {
            sink.lock().unwrap().push(launch.clone());
        })),
        ..env.options()
    });
    let task = env.task();
    restoring
        .start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    let own = OwnAgent {
        restored: true,
        ..OwnAgent::default()
    };
    let updated = restoring
        .start(&task.id, "copilot", None, Some("6"), Some("c-1"), &own)
        .unwrap();

    let script =
        std::fs::read_to_string(env.data_dir.join(format!("agent-launch-{}-6.sh", task.id)));
    assert_eq!(
        script.unwrap(),
        "copilot --allow-all-tools --resume 'c-1'\n"
    );
    assert_eq!(updated.agent.as_deref(), Some("claude-code"));
    let event = last_event(&env.tasks, &task.id);
    assert_eq!(event.kind, "agent.launched");
    assert_eq!(payload(&event, "agent"), json!("copilot"));
    assert_eq!(payload(&event, "terminal"), json!("6"));
    assert_eq!(payload(&event, "restored"), json!(true));
    let last = launches.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        (
            last.terminal.as_str(),
            last.agent.as_str(),
            last.session.as_deref(),
            last.restored
        ),
        ("6", "copilot", Some("c-1"), true)
    );
    assert!(!launches.lock().unwrap()[0].restored);
}

/// A paused agent gets the prompt of the next stage as it comes back on its session.
#[test]
fn goes_on_with_a_session_and_the_prompt_given() {
    let env = Env::new();
    let agents = AgentRunner::new(env.options());
    let task = env.task();
    agents
        .start(
            &task.id,
            "copilot",
            Some("Review it"),
            Some("6"),
            Some("c-1"),
            &OwnAgent::default(),
        )
        .unwrap();

    let script =
        std::fs::read_to_string(env.data_dir.join(format!("agent-launch-{}-6.sh", task.id)));
    assert_eq!(
        script.unwrap(),
        "copilot --allow-all-tools --resume 'c-1' -i 'Review it'\n"
    );
}

#[test]
fn starts_the_next_agent_with_the_mode_saved_in_natamux_json() {
    let env = Env::new();
    // Wired as in main.ts: the mode is read from the settings of the repository's file.
    let repo_dir = env.dir.0.join("repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    let repos = json!([{ "id": "default", "name": "repo", "path": repo_dir }]);
    std::fs::write(env.data_dir.join("repos.json"), repos.to_string()).unwrap();
    let now = Arc::new(std::sync::atomic::AtomicI64::new(0));
    let read = Arc::clone(&now);
    let clock: Clock = Arc::new(move || read.load(Ordering::SeqCst));
    let repos = Repos::new(&env.data_dir, clock);
    let saved = AgentRunner::new(AgentRunnerOptions {
        permission_mode_of: Arc::new(move |repo| repos.settings_of(repo).agents.permission_mode),
        ..env.options()
    });
    let task = env.task();

    saved
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
    std::fs::write(
        repo_dir.join(".natamux.json"),
        json!({ "agents": { "permissionMode": "ask" } }).to_string(),
    )
    .unwrap();
    // The file is looked at once a second.
    now.fetch_add(2000, Ordering::SeqCst);
    saved
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
}

#[test]
fn refuses_a_task_that_waits_for_another_one() {
    let env = Env::new();
    let before = env
        .tasks
        .create_task(Some(&json!({ "title": "First things first" })), &[], true)
        .unwrap();
    let task = env.task();
    env.tasks
        .update_task(&task.id, Some(&json!({ "dependsOn": [before.id] })))
        .unwrap();

    let result = env.runner.start(
        &task.id,
        "claude-code",
        None,
        None,
        None,
        &OwnAgent::default(),
    );

    assert!(matches!(result, Err(AgentError::Waiting { .. })));
    assert_eq!(env.pty_count(), 0);
}

#[test]
fn refuses_an_agent_it_cannot_start_and_a_task_without_a_workspace() {
    let env = Env::new();
    let task = env.task();
    let result = env
        .runner
        .start(&task.id, "unknown", None, None, None, &OwnAgent::default());
    assert!(matches!(result, Err(AgentError::Unknown(_))));

    let bare = env
        .tasks
        .create_task(Some(&json!({ "title": "No workspace" })), &[], true)
        .unwrap();
    let result = env.runner.start(
        &bare.id,
        "claude-code",
        None,
        None,
        None,
        &OwnAgent::default(),
    );
    assert!(matches!(
        result,
        Err(AgentError::Terminal(TerminalError::NoWorkspace(_)))
    ));
    assert_eq!(env.tasks.get_task(&bare.id).unwrap().agent, None);
}

// Node passed `copilotArgs`, which is not ported: `extra_args` is its place.
#[test]
fn starts_copilot_in_interactive_mode_with_the_prompt_and_the_extra_words() {
    let env = Env::new();
    let copilot = AgentRunner::new(AgentRunnerOptions {
        claude_settings_path: "/data/claude-code-settings.json".into(),
        extra_args: HashMap::from([("copilot".to_string(), "--model gpt-5".to_string())]),
        ..env.options()
    });
    let task = env.task_with_workspace("Fix the parser", "");
    copilot
        .start(
            &task.id,
            "copilot",
            Some("Fix it"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
    assert_eq!(
        env.script_text(&task.id),
        "copilot --allow-all-tools --model gpt-5 -i 'Fix it'\n"
    );
    assert_eq!(
        env.tasks.get_task(&task.id).unwrap().agent.as_deref(),
        Some("copilot")
    );
}
