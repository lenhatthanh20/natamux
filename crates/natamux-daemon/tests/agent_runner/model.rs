//! The task's model on the agent's command line.

use natamux_daemon::agents::runner::task_model;

use super::*;

fn with_model(env: &Env, model: &str) -> Task {
    let task = env.task();
    env.tasks
        .update_task(&task.id, Some(&json!({ "model": model })))
        .unwrap()
}

fn task_of(agent: Option<&str>, model: Option<&str>) -> Task {
    Task {
        agent: agent.map(str::to_string),
        model: model.map(str::to_string),
        ..Task::default()
    }
}

#[test]
fn gives_the_model_only_to_the_tasks_own_built_in_agent() {
    let task = task_of(Some("claude-code"), Some("opus"));
    assert_eq!(task_model(&task, "claude-code", None), Some("opus"));
    assert_eq!(task_model(&task, "codex", None), None);
    assert_eq!(task_model(&task_of(None, Some("opus")), "codex", None), Some("opus"));
    assert_eq!(task_model(&task_of(Some("my-tool"), Some("opus")), "my-tool", None), None);
    assert_eq!(task_model(&task_of(Some("codex"), None), "codex", None), None);
}

#[test]
fn leaves_the_model_to_stage_words_that_name_one() {
    let task = task_of(Some("claude-code"), Some("opus"));
    for args in ["--model haiku", "--model=haiku", "-m haiku"] {
        assert_eq!(task_model(&task, "claude-code", Some(args)), None, "{args}");
    }
    assert_eq!(task_model(&task, "claude-code", Some("--verbose")), Some("opus"));
}

#[test]
fn passes_no_model_that_is_not_one_safe_word() {
    let task = task_of(Some("claude-code"), Some("opus; rm -rf"));
    assert_eq!(task_model(&task, "claude-code", None), None);
}

#[test]
fn puts_the_model_after_the_configured_words_and_before_the_stage_words() {
    let env = Env::new();
    let runner = AgentRunner::new(AgentRunnerOptions {
        extra_args: HashMap::from([("claude-code".to_string(), "--foo".to_string())]),
        ..env.options()
    });
    let task = with_model(&env, "opus");
    let own = OwnAgent {
        args: Some("--verbose".into()),
        stage: Some("spec".into()),
        restored: false,
    };

    runner
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();

    let script = env.script_text(&task.id);
    assert!(script.contains("--foo --model 'opus' '--verbose'"), "{script}");
}

#[test]
fn starts_without_a_model_when_the_task_has_none() {
    let env = Env::new();
    let task = env.task();

    env.start(&task, "claude-code", "Go");

    assert!(!env.script_text(&task.id).contains("--model"));
}

#[test]
fn keeps_the_model_of_the_stage_words() {
    let env = Env::new();
    let task = with_model(&env, "opus");
    let own = OwnAgent {
        args: Some("--model haiku".into()),
        stage: Some("spec".into()),
        restored: false,
    };

    env.runner
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();

    let script = env.script_text(&task.id);
    assert!(script.contains("'--model' 'haiku'"), "{script}");
    assert!(!script.contains("'opus'"), "{script}");
}

#[test]
fn gives_no_model_to_a_stages_other_agent() {
    let env = Env::new();
    let task = with_model(&env, "opus");
    env.start(&task, "claude-code", "Go");
    let own = OwnAgent {
        stage: Some("test-spec".into()),
        ..OwnAgent::default()
    };

    env.runner
        .start(&task.id, "codex", Some("Only this"), Some("2"), None, &own)
        .unwrap();

    let other = env.data_dir.join(format!("agent-launch-{}-2.sh", task.id));
    let script = std::fs::read_to_string(other).unwrap();
    assert!(!script.contains("--model"), "{script}");
}

#[test]
fn passes_the_model_in_a_project_that_is_not_trusted_but_not_its_stage_words() {
    let env = Env::new();
    let runner = AgentRunner::new(AgentRunnerOptions {
        trusted_of: Arc::new(|_| false),
        ..env.options()
    });
    let task = with_model(&env, "opus");
    let own = OwnAgent {
        args: Some("--model haiku".into()),
        stage: Some("spec".into()),
        restored: false,
    };

    runner
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();

    let script = env.script_text(&task.id);
    assert!(script.contains("--model 'opus'"), "{script}");
    assert!(!script.contains("haiku"), "{script}");
}

#[test]
fn tells_the_watchers_which_model_it_passed() {
    let env = Env::new();
    let launches: Arc<Mutex<Vec<AgentLaunch>>> = Arc::default();
    let seen = Arc::clone(&launches);
    let runner = AgentRunner::new(AgentRunnerOptions {
        on_launch: Some(Arc::new(move |launch: &AgentLaunch| {
            seen.lock().unwrap().push(launch.clone());
        })),
        ..env.options()
    });
    let task = with_model(&env, "sonnet[1m]");

    runner
        .start(
            &task.id,
            "claude-code",
            Some("Go"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();

    let launches = launches.lock().unwrap();
    assert_eq!(launches[0].model.as_deref(), Some("sonnet[1m]"));
}
