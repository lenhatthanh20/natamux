//! T-0373 Part 5 (F2, F5): agent words must not undo the chosen permission mode. In `ask` a word
//! that turns off approvals fails the start; Strict mode never starts an agent in auto.

use natamux_agents::launch::{LAUNCHABLE_AGENTS, auto_flags};
use natamux_core::security::{Forced, SecurityMode, StrictSettings, force};
use natamux_daemon::agents::launch_policy_check::{OTHER_NOTICE, POLICY_BLOCKED_EVENT};
use natamux_proto::agents::custom::CustomAgent;

use super::*;

/// Parts of auto lines that must never be in a Strict line.
const NEVER_IN_STRICT: [&str; 3] = [
    "--ask-for-approval never",
    "network_access",
    "GOOSE_MODE=auto",
];

fn runner_in(env: &Env, mode: &'static str, extra: &[(&str, &str)]) -> AgentRunner {
    AgentRunner::new(AgentRunnerOptions {
        permission_mode_of: Arc::new(move |_| mode.to_string()),
        extra_args: extra
            .iter()
            .map(|(agent, words)| (agent.to_string(), words.to_string()))
            .collect(),
        ..env.options()
    })
}

fn events_of(tasks: &TaskService, task_id: &str, kind: &str) -> Vec<TaskEvent> {
    tasks
        .list_events(task_id)
        .unwrap()
        .into_iter()
        .map(|stored| stored.event)
        .filter(|event| event.kind == kind)
        .collect()
}

fn stage(args: &str) -> OwnAgent {
    OwnAgent {
        args: Some(args.into()),
        stage: Some("spec".into()),
        restored: false,
    }
}

/// Strict mode with every agent allowed, so the auto rule is seen apart from the ask guarantee.
fn strict_all_agents() -> Forced {
    force(SecurityMode::Strict(StrictSettings {
        require_ask_guarantee: false,
        ..StrictSettings::default()
    }))
}

fn is_blocked(result: Result<Task, AgentError>) -> bool {
    matches!(result, Err(AgentError::Untrusted(_)))
}

#[test]
fn ask_mode_fails_a_start_with_a_forbidden_word_and_starts_nothing() {
    let env = Env::new();
    let task = env.task();
    let runner = runner_in(&env, "ask", &[("codex", "--yolo")]);
    let own = OwnAgent::default();
    let result = runner.start(&task.id, "codex", None, None, None, &own);
    assert!(is_blocked(result));

    let own = stage("--permission-mode bypassPermissions");
    let id = task.id.as_str();
    let result = runner.start(id, "claude-code", None, None, None, &own);
    assert!(is_blocked(result));

    assert_eq!(env.pty_count(), 0, "no terminal ran");
    assert!(!env.script(&task.id).exists(), "no launch script");
    let blocked = events_of(&env.tasks, &task.id, POLICY_BLOCKED_EVENT);
    assert_eq!(blocked.len(), 2);
    assert_eq!(payload(&blocked[0], "word"), json!("--yolo"));
    assert_eq!(payload(&blocked[0], "source"), json!("NATAMUX_CODEX_ARGS"));
    let reason = payload(&blocked[0], "reason");
    assert!(reason.as_str().unwrap().contains("--yolo"), "{reason}");
    assert_eq!(
        payload(&blocked[1], "word"),
        json!("--permission-mode bypassPermissions")
    );
    assert_eq!(payload(&blocked[1], "source"), json!("stage args"));
}

#[test]
fn ask_mode_starts_with_plain_words() {
    let env = Env::new();
    let task = env.task();
    let runner = runner_in(&env, "ask", &[("claude-code", "--verbose")]);
    let own = stage("--model x");
    runner
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' --verbose '--model' 'x' 'Go'\n"
    );
    assert!(events_of(&env.tasks, &task.id, POLICY_BLOCKED_EVENT).is_empty());
}

#[test]
fn auto_mode_keeps_the_launch_line_as_it_was() {
    let env = Env::new();
    let task = env.task();
    let runner = runner_in(&env, "auto", &[]);
    let own = stage("--permission-mode bypassPermissions");
    runner
        .start(&task.id, "claude-code", Some("Go"), None, None, &own)
        .unwrap();
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' --permission-mode auto \
         '--permission-mode' 'bypassPermissions' 'Go'\n"
    );
    assert!(events_of(&env.tasks, &task.id, POLICY_BLOCKED_EVENT).is_empty());
}

#[test]
fn strict_starts_no_agent_in_auto_also_on_resume() {
    let env = Env::new();
    let task = env.task();
    let runner = runner_in(&env, "auto", &[]);
    let _strict = strict_all_agents();
    let own = OwnAgent::default();
    for agent in LAUNCHABLE_AGENTS {
        runner
            .start(&task.id, agent, Some("Go"), None, None, &own)
            .unwrap();
        let line = env.script_text(&task.id);
        let auto = auto_flags(agent).join(" ");
        assert!(auto.is_empty() || !line.contains(&auto), "{agent}: {line}");
        for word in NEVER_IN_STRICT {
            assert!(!line.contains(word), "{agent}: {line}");
        }
    }
    let old_session = Some("abc-123");
    runner
        .start(&task.id, "claude-code", None, None, old_session, &own)
        .unwrap();
    let line = env.script_text(&task.id);
    assert!(!line.contains("--permission-mode auto"), "{line}");
    assert!(line.contains("--permission-mode default"), "{line}");
}

#[test]
fn strict_checks_the_words_even_when_the_project_says_auto() {
    let env = Env::new();
    let task = env.task();
    let skip = [("claude-code", "--dangerously-skip-permissions")];
    let runner = runner_in(&env, "auto", &skip);
    let _strict = force(SecurityMode::Strict(StrictSettings::default()));
    let own = OwnAgent::default();
    let id = task.id.as_str();
    let result = runner.start(id, "claude-code", None, None, None, &own);
    assert!(is_blocked(result));
    assert_eq!(env.pty_count(), 0);
    let blocked = events_of(&env.tasks, &task.id, POLICY_BLOCKED_EVENT);
    assert_eq!(
        payload(&blocked[0], "word"),
        json!("--dangerously-skip-permissions")
    );
}

// The only test of this binary that sets the custom agents (they are the process's).
#[test]
fn an_agent_on_base_other_runs_in_ask_with_a_notice_and_never_in_strict() {
    natamux_agents::custom::set_custom_agents(vec![CustomAgent {
        id: "mytool".into(),
        base: "other".into(),
        command: "mytool".into(),
    }]);
    let env = Env::new();
    let task = env.task();
    let runner = runner_in(&env, "ask", &[]);
    let own = OwnAgent::default();
    runner
        .start(&task.id, "mytool", Some("Go"), None, None, &own)
        .unwrap();
    assert_eq!(env.pty_count(), 1);
    assert_eq!(env.script_text(&task.id), "mytool 'Go'\n");
    let notices = events_of(&env.tasks, &task.id, "workspace.warning");
    assert_eq!(payload(&notices[0], "message"), json!(OTHER_NOTICE));

    {
        let _strict = strict_all_agents();
        let result = runner.start(&task.id, "mytool", None, None, None, &own);
        assert!(is_blocked(result));
    }
    {
        let _strict = force(SecurityMode::Strict(StrictSettings::default()));
        let result = runner.start(&task.id, "mytool", None, None, None, &own);
        assert!(is_blocked(result));
    }
    assert_eq!(env.pty_count(), 1, "Strict started nothing");
    let blocked = events_of(&env.tasks, &task.id, POLICY_BLOCKED_EVENT);
    assert_eq!(blocked.len(), 1);
}

#[test]
fn a_blocked_start_writes_one_audit_line_without_the_word_or_reason() {
    let (sink, lines) = natamux_core::privacy::audit::catcher();
    let _audit = natamux_core::privacy::audit::force(sink);
    let env = Env::new();
    let task = env.task();
    let runner = runner_in(&env, "ask", &[("codex", "--yolo")]);
    let result = runner.start(&task.id, "codex", None, None, None, &OwnAgent::default());
    assert!(is_blocked(result));
    let lines = lines.lock().unwrap().clone();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], ["agent.blocked", task.id.as_str(), "policy"]);
    assert!(!format!("{lines:?}").contains("yolo"));
}
