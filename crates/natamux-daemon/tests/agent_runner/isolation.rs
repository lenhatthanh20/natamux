//! T-0373 Part 7 (F1): with isolation required only Claude Code starts, and only where isolation
//! can run. A refused start writes no script and starts no shell.

use natamux_core::security::IsolationSettings;
use natamux_daemon::terminals::isolation::Isolation;

use super::*;

fn isolated(env: &Env, support: Result<u32, String>) -> AgentRunner {
    let runner = AgentRunner::new(env.options());
    runner.isolate(Arc::new(Isolation {
        settings: IsolationSettings {
            required: true,
            ..IsolationSettings::default()
        },
        data_dir: env.data_dir.clone(),
        bin_dir: None,
        home: env.dir.0.join("home"),
        support,
        git_user: None,
    }));
    runner
}

fn start(runner: &AgentRunner, task: &Task, agent: &str) -> Result<Task, AgentError> {
    runner.start(
        &task.id,
        agent,
        Some("Go"),
        None,
        None,
        &OwnAgent::default(),
    )
}

#[test]
fn another_agent_is_refused_before_any_shell_or_script() {
    let env = Env::new();
    let task = env.task();
    let runner = isolated(&env, Ok(4));
    for agent in ["codex", "gemini", "cursor", "copilot"] {
        let refused = start(&runner, &task, agent);
        assert!(matches!(refused, Err(AgentError::Untrusted(_))), "{agent}");
    }
    assert_eq!(env.pty_count(), 0);
    assert!(!env.script(&task.id).exists());
    assert!(!env.data_dir.join("sandbox").join(&task.id).exists());
}

#[test]
fn claude_code_starts_with_its_script_in_the_task_sandbox_folder() {
    let env = Env::new();
    let task = env.task();
    let runner = isolated(&env, Ok(4));
    start(&runner, &task, "claude-code").unwrap();
    assert_eq!(env.pty_count(), 1);
    let script = env
        .data_dir
        .join("sandbox")
        .join(&task.id)
        .join(format!("agent-launch-{}.sh", task.id));
    assert!(script.exists(), "no script at {script:?}");
    assert!(!env.script(&task.id).exists());
}

#[test]
fn no_agent_starts_where_isolation_cannot_run() {
    let env = Env::new();
    let task = env.task();
    let runner = isolated(&env, Err("Isolation needs Linux with Landlock".into()));
    let refused = start(&runner, &task, "claude-code");
    assert!(matches!(refused, Err(AgentError::Untrusted(reason)) if reason.contains("Landlock")));
    assert_eq!(env.pty_count(), 0);
}

#[test]
fn without_isolation_any_agent_starts_as_before() {
    let env = Env::new();
    let task = env.task();
    let runner = AgentRunner::new(env.options());
    assert!(runner.isolation().is_none());
    start(&runner, &task, "codex").unwrap();
    assert_eq!(env.pty_count(), 1);
}

#[test]
fn the_task_token_ends_with_the_last_terminal_under_isolation_only() {
    let env = Env::new();
    let task = env.task();
    let runner = isolated(&env, Ok(4));
    start(&runner, &task, "claude-code").unwrap();
    let tokens = env.terminals.agent_tokens();
    let old = tokens.token_for(&task.id);
    runner.forget_token_if_last(&task.id);
    assert_eq!(tokens.task_of(&old).as_deref(), Some(task.id.as_str()));
    env.terminals.close(&task.id);
    runner.forget_token_if_last(&task.id);
    assert_eq!(tokens.task_of(&old), None);
    let plain = AgentRunner::new(env.options());
    let kept = tokens.token_for(&task.id);
    plain.forget_token_if_last(&task.id);
    assert_eq!(tokens.task_of(&kept).as_deref(), Some(task.id.as_str()));
}
