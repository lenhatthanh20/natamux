//! T-0373 Part 1 (F2): the launch gate in Strict mode. A worktree that is not approved as it is
//! now starts no agent on any path, and an approved start asks before each action.

use natamux_agents::launch::LAST_SESSION;
use natamux_agents::trust::{TrustPaths, trust_workspace};
use natamux_core::security::{Forced, SecurityMode, StrictSettings, force};
use natamux_daemon::projects::repo_trust_worktree::{TrustState, TrustedOf, trust_always};

use super::*;

fn strict() -> Forced {
    force(SecurityMode::Strict(StrictSettings::default()))
}

fn runner_with(env: &Env, trusted_of: TrustedOf) -> AgentRunner {
    AgentRunner::new(AgentRunnerOptions {
        trusted_of,
        ..env.options()
    })
}

/// Trust markers of Cursor and Copilot for the env's worktree, as an earlier start left them.
fn old_markers(env: &Env) -> (TrustPaths, PathBuf) {
    let paths = trust_paths(|_| None, &env.data_dir);
    std::fs::create_dir_all(paths.copilot_config.parent().unwrap()).unwrap();
    std::fs::write(&paths.copilot_config, "{}").unwrap();
    trust_workspace("cursor", &env.workspace, &paths);
    trust_workspace("copilot", &env.workspace, &paths);
    let slug = std::fs::read_dir(&paths.cursor_projects)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let marker = slug.join(".workspace-trusted");
    assert!(marker.exists());
    (paths, marker)
}

/// The five ways an agent starts: new launch, resume, restore after a restart, a paused shell
/// reused, reopen.
fn starts() -> Vec<(
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    OwnAgent,
)> {
    let restored = OwnAgent {
        restored: true,
        ..OwnAgent::default()
    };
    vec![
        ("new", None, None, OwnAgent::default()),
        ("resume", None, Some("abc-123"), OwnAgent::default()),
        ("restore", None, Some("abc-123"), restored),
        ("paused reuse", Some("3"), None, OwnAgent::default()),
        ("reopen", None, Some(LAST_SESSION), OwnAgent::default()),
    ]
}

#[test]
fn a_changed_worktree_starts_nothing_on_any_start_path_and_takes_back_old_trust() {
    let env = Env::new();
    let (paths, marker) = old_markers(&env);
    let runner = runner_with(&env, trust_always(false));
    let task = env.task();
    let _strict = strict();

    for (name, terminal, session, own) in starts() {
        let result = runner.start(&task.id, "claude-code", None, terminal, session, &own);
        assert!(matches!(result, Err(AgentError::Untrusted(_))), "{name}");
    }

    assert_eq!(env.pty_count(), 0, "no terminal ran");
    assert!(!env.script(&task.id).exists(), "no launch script");
    assert!(!env.data_dir.join("agent-launch-T-0001-3.sh").exists());
    assert!(!marker.exists(), "the old Cursor marker is gone");
    let copilot = std::fs::read_to_string(&paths.copilot_config).unwrap();
    assert!(!copilot.contains(&env.workspace), "{copilot}");
}

#[test]
fn personal_mode_starts_a_changed_worktree_limited_as_before() {
    let env = Env::new();
    let runner = runner_with(&env, trust_always(false));
    let task = env.task();
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
    assert_eq!(env.pty_count(), 1);
    assert_eq!(
        env.script_text(&task.id),
        "claude --settings '/data/claude code.json' 'Go'\n"
    );
}

#[test]
fn strict_refuses_an_agent_that_cannot_be_held_to_asking() {
    let env = Env::new();
    let task = env.task();
    let _strict = strict();
    for agent in ["cursor", "copilot", "aider"] {
        let result = env
            .runner
            .start(&task.id, agent, None, None, None, &OwnAgent::default());
        assert!(matches!(result, Err(AgentError::Untrusted(_))), "{agent}");
    }
    assert_eq!(env.pty_count(), 0);
}

#[test]
fn an_approved_strict_start_asks_and_checks_the_digest_before_the_agent() {
    let env = Env::new();
    let task = env.task();
    let _strict = strict();
    let verify = "natamux repo verify-launch --task 'T-0001' --digest 'always' && ";

    env.start(&task, "claude-code", "Go");
    let line = "claude --settings '/data/claude code.json' --permission-mode default 'Go'\n";
    assert_eq!(env.script_text(&task.id), format!("{verify}{line}"));

    env.start(&task, "codex", "Go");
    let codex = env.script_text(&task.id);
    assert!(codex.starts_with(verify), "{codex}");
    assert!(
        codex.contains(" --ask-for-approval on-request --sandbox workspace-write"),
        "{codex}"
    );
    assert!(!codex.contains("never"), "{codex}");
    assert!(codex.contains("--dangerously-bypass-hook-trust"), "{codex}");

    env.start(&task, "goose", "Go");
    let goose = env.script_text(&task.id);
    assert!(
        goose.starts_with(&format!("{verify}GOOSE_MODE=approve goose session")),
        "{goose}"
    );
}

#[test]
fn the_windows_line_runs_the_agent_only_after_the_check_passed() {
    let env = Env::new();
    let task = env.task();
    let (windows, ptys) = env.windows("C:\\data\\settings.json", None);
    let _strict = strict();
    windows
        .start(
            &task.id,
            "claude-code",
            Some("Go"),
            None,
            None,
            &OwnAgent::default(),
        )
        .unwrap();
    let typed = ptys.lock().unwrap()[0].written().join("");
    let check = "natamux repo verify-launch --task 'T-0001' --digest 'always'; \
                 if ($LASTEXITCODE -eq 0) { claude ";
    assert!(typed.contains(check), "{typed}");
}

/// Gives `first` for the first `calls` asks, then another digest: an edit in between.
fn edited_after(calls: usize) -> TrustedOf {
    let count = Arc::new(AtomicU32::new(0));
    Arc::new(move |_: Option<&str>, _: Option<&Path>| {
        let seen = count.fetch_add(1, Ordering::SeqCst) as usize;
        let digest = if seen < calls { "approved" } else { "edited" };
        TrustState::Trusted {
            digest: digest.into(),
        }
    })
}

#[test]
fn an_edit_between_the_gate_and_the_run_aborts_the_start() {
    for calls in [1, 2] {
        let env = Env::new();
        let runner = runner_with(&env, edited_after(calls));
        let task = env.task();
        let result = runner.start(
            &task.id,
            "claude-code",
            None,
            None,
            None,
            &OwnAgent::default(),
        );
        assert!(matches!(result, Err(AgentError::Untrusted(_))), "{calls}");
        assert_eq!(env.pty_count(), 0, "{calls}");
    }
    let env = Env::new();
    let runner = runner_with(&env, edited_after(3));
    let task = env.task();
    let started = runner.start(
        &task.id,
        "claude-code",
        None,
        None,
        None,
        &OwnAgent::default(),
    );
    assert!(started.is_ok());
    assert_eq!(env.pty_count(), 1);
}

#[cfg(unix)]
#[test]
fn a_failed_check_stops_the_script_before_the_agent() {
    use std::os::unix::fs::PermissionsExt;

    let env = Env::new();
    let task = env.task();
    {
        let _strict = strict();
        env.start(&task, "claude-code", "Go");
    }
    let bin = env.dir.0.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let mark = env.dir.0.join("agent-ran");
    let program = |name: &str, body: &str| {
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    program("claude", "touch \"$MARK\"");
    let run = |code: u8| {
        program("natamux", &format!("exit {code}"));
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let script = env.script(&task.id);
        std::process::Command::new("sh")
            .arg("-c")
            .arg(format!(". '{}'", script.display()))
            .env("PATH", path)
            .env("MARK", &mark)
            .status()
            .unwrap();
    };

    run(1);
    assert!(!mark.exists(), "the agent ran after a failed check");
    run(0);
    assert!(mark.exists(), "the agent did not run after a passed check");
}
